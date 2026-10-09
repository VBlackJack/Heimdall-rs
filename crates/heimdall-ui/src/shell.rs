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

use heimdall_core::settings::{AgentPreference, CtrlKTerminal, CtrlVPaste, ExecutionPolicy};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use heimdall_app::files::{
    EntryKind, FilesError, FilesKey, Side, list_local, list_remote, plan_transfer,
};
use heimdall_app::ftp_driver::ftp_events;
use heimdall_app::gateway_draft::{GATEWAY_FIELDS, GatewayDraft};
use heimdall_app::local_driver::local_events;
use heimdall_app::profile_draft::{
    DraftError, DraftProtocol, ProfileChoice, ProfileDraft, ProfileField, ProfileToggle,
    SavedSecret,
};
use heimdall_app::rdp_driver::rdp_events;
use heimdall_app::session_log::{
    OperationJournal, copy_remote_recorded, file_operation_recorded, move_remote_recorded,
    transfer_events_recorded,
};
use heimdall_app::telnet_driver::telnet_events;
use heimdall_app::tools::SidebarTab;
use heimdall_app::tunnel_driver::tunnel_events;
use heimdall_app::type_ahead::TYPE_AHEAD_RESET;
use heimdall_app::update_check::{Failure as UpdateFailure, GitHubSource, Outcome};
use heimdall_app::vnc_driver::vnc_events;
use heimdall_app::winrm_driver::winrm_events;
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, BroadcastMessage, CertificateContext,
    ConnectionEvent, DesktopPane, Dialog, Effect, ElevatedPane, ElevatedState, FilesMessage,
    FilterMessage, FloatId, FloatMessage, FolderMessage, FolderNaming, LONG_MASTER_PASSWORD_CHARS,
    LocalConfirmation, MIN_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CLASSES,
    Message as AppMessage, NameAction, PastePreview, Phase, PinDialog, PinFailure, PinMessage,
    PinMode, PostConnectConfirmation, PostConnectProgress, ProfileMenuMessage, Prompt,
    ProviderMessage, Purpose, QuestionId, QuestionKind, Retry, SaveState, ScriptConfirmation,
    SelectionMessage, SessionState, SettingsMessage, SpecialKeys, SystemCredentials, Tab, TabGroup,
    TabId, TabMenuMessage, TabProfile, TreeRow, TrustedKeysMessage, TunnelMessage, UiError,
    VaultDialog, VaultJob, VaultMode, VaultProblem, VaultStatus, VncQuality, connection_events,
    master_password_problem, open_vault, server_text,
};
use heimdall_app::{ToolsMessage, VaultHelloMessage, VaultTicket, vault_hello, windows_hello};
use heimdall_core::folder::FolderError;
use heimdall_core::paths::{self, KNOWN_HOSTS_FILE_NAME, PROFILES_FILE_NAME};
use heimdall_core::pin::{MAX_PIN_DIGITS, MIN_PIN_DIGITS, PinProblem};
use heimdall_core::profile::{ProfileId, RdpProfile, SshGateway, SshMode, display_address};
use heimdall_core::settings::Language;
use heimdall_core::settings::{AppTheme, BroadcastScope, ColorScheme};
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
use iced::{
    Color, Element, Length, Point, Rectangle, Subscription, Task, Theme, event, keyboard, window,
};
use zeroize::Zeroizing;

mod files_input;
mod floating_files;
mod floating_find;
mod settings_page;

pub use settings_page::search_field_id as settings_search_field_id;

use crate::desktop_view::DesktopView;
use crate::dialog_parts::{self, Severity};
use crate::files_view;
use crate::finder::Finder;
use crate::floating_view::{FloatEvent, FloatingWindow};
use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::palette::Palette;
pub use crate::profile_tabs::ProfileTab;
use crate::report;
use crate::search_keys::SearchKeys;
pub use crate::session_settings::SessionField;
use crate::settings_rows::{SettingRow, ToolPath};
use crate::split_view::{self, Shape, SplitView};
use crate::styles;
use crate::terminal_view::TerminalView;
use crate::terminal_view::font::TerminalFont;
use crate::terminal_view::keys::{
    WindowShortcut, Zoom, ctrl_letter, is_lock_key, is_search_key, window_shortcut,
};
use crate::texts;
use crate::tokens::{font_size, radius, spacing};
use crate::tree_view::{
    self, CursorSpot, CursorTracker, SplitEntries, TabMenuState, TranscriptEntry, TreeMenu,
};
use crate::trusted_keys_view::{HostKeyColumn, HostKeySort, TrustedList};
use heimdall_app::split::{Axis, Layout as SplitLayout, MAX_PANES, Placement, SplitMessage};

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

/// Space between the terminal and the panels around it, in logical pixels.
const TERMINAL_MARGIN: f32 = 6.0;

/// Width of a question or dialog card, in logical pixels.
const CARD_WIDTH: f32 = 520.0;

/// Widest a dialog holding a table grows.
const WIDE_CARD_WIDTH: f32 = 1000.0;

/// Size of the profile form, as the C# server dialog's window: 650 by 750.
const FORM_SIZE: iced::Size = iced::Size::new(650.0, 750.0);

/// Height of the sidebar's heading, as the C# `SidebarTabStyle`'s, its underline in.
const SIDEBAR_TAB_HEIGHT: f32 = 32.0;

/// Thickness of the underline of the sidebar's heading, as the C# `SidebarTabStyle`'s.
const SIDEBAR_TAB_UNDERLINE: f32 = 2.0;

/// Room around the sidebar's text field, as the C# search box's `10,6`, more at its right
/// where its clear button sits.
const SEARCH_PADDING: iced::Padding = iced::Padding {
    top: 6.0,
    right: 28.0,
    bottom: 6.0,
    left: 10.0,
};

/// Room around the search box's clear button, as the C#'s.
const SEARCH_CLEAR_PADDING: f32 = 4.0;

/// Side of the search box's clear button, as the C#'s glyph in its padding.
const SEARCH_CLEAR_SIDE: f32 = 20.0;

/// Space between the search box and the filter button, as the C#'s margin.
const SEARCH_GAP: f32 = 6.0;

/// Space between the sidebar's icon buttons, as the C#'s margins.
const TOOL_GAP: f32 = 2.0;

/// Side of the dot on the filter button while a filter is on, as the C#'s.
const FILTER_DOT_SIDE: f32 = 6.0;

/// Room around the tab strip, as the C#'s: none below, where the tabs meet the session.
const TAB_STRIP_PADDING: iced::Padding = iced::Padding {
    top: 8.0,
    right: 8.0,
    bottom: 0.0,
    left: 8.0,
};

/// Space between two tabs, as the C# `ThemedTabItemStyle`'s right margin.
const TAB_GAP: f32 = 4.0;

/// Room inside a tab, around its heading and underline, as the C# `ThemedTabItemStyle`'s
/// `16,10`.
const TAB_PADDING: [f32; 2] = [10.0, 16.0];

/// Room around a tab's heading, as the C# tab header's `10,6`.
const TAB_HEADING_PADDING: [f32; 2] = [6.0, 10.0];

/// Thickness of a selected tab's underline, as the C#'s.
const TAB_UNDERLINE: f32 = 3.0;

/// Height of a tab's error badge, and its least width, as the C# `MinWidth` and `Height`.
const ERROR_BADGE_SIDE: f32 = 18.0;

/// Space on each side of the count in a tab's error badge, as the C# badge's padding.
const ERROR_BADGE_PADDING_X: f32 = 5.0;

/// Space between a tab's name and its error badge, as the C# badge's left margin.
const ERROR_BADGE_GAP: f32 = 6.0;

/// Side of a tab's close button, as the C#'s 20 by 20.
const TAB_CLOSE_SIDE: f32 = 20.0;

/// Side of the pin before a pinned tab's title, as the C#'s glyph.
const TAB_PIN_SIDE: f32 = 10.0;

/// Width of the port column beside the server field, as in the C# dialog.
const PORT_FIELD_WIDTH: f32 = 150.0;

/// Side of a protocol's icon on its card in the new session's picker, as the C#'s.
const PICKER_ICON_SIDE: f32 = 28.0;

/// Cards to a row in the new session's picker, as the C# `UniformGrid`.
const PICKER_COLUMNS: usize = 4;

/// Padding of the profile form's protocol, as the C# badge's 10 by 4.
const CHIP_PADDING: [f32; 2] = [4.0, 10.0];

/// Padding of a tab control's headers, as the C#'s 8 above and on the sides.
const TAB_HEADERS_PADDING: iced::Padding = iced::Padding {
    top: spacing::SM,
    right: spacing::SM,
    bottom: 0.0,
    left: spacing::SM,
};

/// Space between two sections of the profile form, as the C# cards' margin and padding.
const SECTION_GAP: f32 = spacing::LG + spacing::MD;

/// The boxes of a `WinRM` profile drawn under its port, as the C# General tab draws them.
const WINRM_TRANSPORT: [ProfileToggle; 2] =
    [ProfileToggle::UseSsl, ProfileToggle::SkipCertificateCheck];

/// Where the SOCKS proxy listens: this computer's loopback address, as in the C# Heimdall.
const LOOPBACK: &str = "127.0.0.1";

/// The port that opens none, the placeholder of the forwarded ports' fields.
const PORT_OFF: u16 = 0;

/// Tallest the list of skipped profiles grows before it scrolls, in logical pixels.
const SKIPPED_LIST_HEIGHT: f32 = 200.0;

/// Height the command of a local profile scrolls within, however long it is.
const LOCAL_COMMAND_HEIGHT: f32 = 240.0;

/// Tallest the text of a paste asked about grows before it scrolls, as the C# preview box.
const PASTE_PREVIEW_HEIGHT: f32 = 280.0;

/// How often a waiting session's countdown is drawn anew.
const COUNTDOWN_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Width of the outline round the tab a dragged tab would take the place of.
const TAB_DROP_EDGE: f32 = 2.0;

/// Width of the border of the "Drop to split" overlay, as the C# `ContentDropZone`'s.
const SPLIT_DROP_EDGE: f32 = 2.0;

/// How much of the session under the "Drop to split" overlay it hides.
const SPLIT_DROP_SHADE: f32 = 0.8;

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
        'z' => Some(TreeShortcut::Undo),
        'b' => Some(TreeShortcut::ToggleSidebar),
        _ => None,
    }
}

/// A character typed that no widget took, for the type-ahead of the tree or of a Files
/// list: printable, without Ctrl, Alt or the logo key.
fn type_ahead(text: Option<&str>, modifiers: keyboard::Modifiers) -> Option<Message> {
    crate::terminal_view::keys::typed_text(text, modifiers)
        .map(|text| Message::TypeAhead(text.to_owned()))
}

/// `event::listen_with` over the listener `$listener`, each message paired with the window
/// its event came from, for [`in_main_window`] to keep the main window's. `listen_with`
/// takes a function pointer, which carries no state: the pairing is a closure capturing
/// nothing, written out for each listener.
macro_rules! window_tagged {
    ($listener:path) => {
        event::listen_with(|event, status, window| {
            $listener(event, status, window).map(|message| (window, message))
        })
    };
}

/// Whether an event of `window` is the main window's: the window `main` names, or any
/// while none is named, as in tests, which open none.
fn from_main(main: Option<window::Id>, window: window::Id) -> bool {
    main.is_none_or(|main| main == window)
}

/// The message of an event from the main window, none from another; what
/// [`in_main_window`] keeps of a subscription given the main window's identifier.
fn main_only<T>((main, (window, message)): (Option<window::Id>, (window::Id, T))) -> Option<T> {
    from_main(main, window).then_some(message)
}

/// `events`, each paired with the window it came from, kept to the main window's: a
/// subscription's map takes no closure capturing `main`, so `main` rides with the events.
fn in_main_window(
    events: Subscription<(window::Id, Message)>,
    main: Option<window::Id>,
) -> Subscription<Message> {
    events.with(main).filter_map(main_only)
}

/// The main window, or, while none is named, as in tests, the window opened last: the
/// window the dialogs are held by, the screenshot taken of, and the place kept of.
pub(crate) fn main_window_task(main: Option<window::Id>) -> Task<Option<window::Id>> {
    match main {
        Some(id) => Task::done(Some(id)),
        None => window::latest(),
    }
}

/// Whether files dropped from Explorer on `tab` go to it: a Files tab with its session open,
/// as the C# tab takes them.
fn takes_drops(tab: &Tab) -> bool {
    tab.phase == Phase::Connected
        && tab
            .files
            .as_ref()
            .is_some_and(|files| files.client.is_some())
}

/// "Drop files to upload", over a Files tab while files are dragged over its window.
fn drop_layer<'a>() -> Element<'a, Message> {
    opaque(
        container(
            container(text(fl!("ui-files-drop-overlay")).size(font_size::TITLE))
                .padding(spacing::MD)
                .style(container::bordered_box),
        )
        .center(Length::Fill),
    )
}

/// Gives `window` the focus, restored first when it is minimized.
fn focus_window(window: window::Id) -> Task<Message> {
    window::is_minimized(window).then(move |minimized| {
        if minimized == Some(true) {
            window::minimize(window, false).chain(window::gain_focus(window))
        } else {
            window::gain_focus(window)
        }
    })
}

/// Where a tab dragged goes when let go, said over the part of the window `zone` it takes,
/// as the C# `ContentDropZone`: the window shaded under an outline, `label` in the middle.
fn drop_zone_overlay(label: String, zone: Rectangle) -> Element<'static, Message> {
    let label = text(label)
        .size(font_size::TITLE)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().primary.strong.color),
        });
    let shaded = container(label)
        .width(Length::Fixed(zone.width))
        .height(Length::Fixed(zone.height))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(
                    Color {
                        a: SPLIT_DROP_SHADE,
                        ..palette.background.base.color
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: palette.primary.strong.color,
                    width: SPLIT_DROP_EDGE,
                    radius: radius::SM.into(),
                },
                ..container::Style::default()
            }
        });
    pin(shaded)
        .x(zone.x)
        .y(zone.y)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
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
            // Ctrl+W no terminal and no desktop took: the session shown closes, once no
            // field has the keyboard either, which is asked first.
            if crate::terminal_view::keys::is_ctrl_w(&key, physical_key, modifiers) {
                return (!repeat).then_some(Message::CloseKey);
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
    /// The header of a column of the trusted SSH host keys clicked: sorted by it.
    HostKeySort(HostKeyColumn),
    /// A language chosen on the Settings page.
    LanguageChosen(Language),
    /// F11: the window full screen, showing the session only, or back.
    ToggleFullscreen,
    /// The window opened: its screen's density is asked for.
    WindowOpened(window::Id),
    /// The main window closed: the application ends, as it did with its only window.
    MainWindowClosed,
    /// A later launch asked this instance to come forward, as the C# activation event: the
    /// main window restored and focused.
    BringForward,
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
    /// A field of the bulk password dialog changed.
    BulkPasswordField {
        /// Field: the password, then again.
        index: usize,
        /// New content.
        value: String,
    },
    /// Move to field `index` of the bulk password dialog.
    FocusBulkPasswordField(usize),
    /// Hand the password typed twice in the bulk password dialog to the core.
    SubmitBulkPassword,
    /// Show this tab of the Settings page.
    SettingsTab(SettingsTab),
    /// Show this tab of the profile form.
    ProfileTab(ProfileTab),
    /// The Settings page's search changed: its rows are filtered by what it holds.
    SettingsSearch(String),
    /// "Find modified settings": the search set to the "Modified" marker's word.
    FindModifiedSettings,
    /// A row's "Reset": its default put back.
    ResetSetting(SettingRow),
    /// A line of the security overview's "Go to setting": the row shown, outlined.
    GoToSetting(SettingRow),
    /// Pick the SSH key of the profile form in the system's open dialog.
    BrowseKeyFile,
    /// A "Browse..." button: the system's dialog for this path.
    Browse(crate::browse::BrowseTarget),
    /// The path picked in that dialog.
    Browsed(crate::browse::BrowseTarget, String),
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
    /// A way of a tab menu's "Split...": the menu closes, Quick Connect opens to choose what
    /// is merged into `host`, as the C# palette's split mode.
    SplitPalette {
        /// The tab split.
        host: TabId,
        /// How the two are placed.
        axis: Axis,
    },
    /// Copy the report of a tab's failure, as the C# card's "Copy error".
    CopyError(TabId),
    /// Copy the anonymized report of tab's failure: no server, account nor message.
    CopyAnonymousError(TabId),
    /// A second passed while a tab waits to open again: its countdown is drawn anew.
    Tick,
    /// Time to measure how long the computer has had no input, for the idle auto-lock.
    IdleTick,
    /// Whether Windows high contrast is on, looked at again: the window follows it.
    SystemHighContrast(bool),
    /// A key, a click or a wheel turn in one of the windows, where the computer's idle time
    /// cannot be read.
    UserInput,
    /// Shift, Ctrl, Alt or the logo key pressed or released.
    Modifiers(keyboard::Modifiers),
    /// A click on a profile of the tree: it alone selected, or, with Ctrl, added or taken,
    /// or, with Shift, all from the last one clicked.
    TreeClick(ProfileId),
    /// A click in the session shown: the keyboard goes back to it from the tree.
    ContentFocus,
    /// Ctrl+E or Ctrl+N uncaptured by any widget, or the Quick Connect button.
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
    /// A Files pane's columns resized by a separator of their header, dragged or double
    /// clicked: kept for the session, as the C# list keeps them.
    FileColumns {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: heimdall_app::files::Side,
        /// The widths of its columns besides the name.
        widths: crate::files_view::ColumnWidths,
    },
    /// The pointer came over a place in a Files tab's panes.
    FilesHover(crate::files_drag::Spot),
    /// The pointer left it.
    FilesHoverLeft(crate::files_drag::Spot),
    /// The left button went down, wherever: a press on a Files tab's entry, or on a tab,
    /// starts a drag.
    PointerPressed,
    /// Ctrl+W left by every widget: the session shown closes unless a field has the
    /// keyboard.
    CloseKey,
    /// Whether a field had the keyboard when Ctrl+W was pressed.
    CloseKeyFocus(bool),
    /// The pointer came over a tab of the tab bar.
    TabHover(TabId),
    /// The pointer left it.
    TabHoverLeft(TabId),
    /// The pointer moved, a press on a tab held.
    TabDragMoved(Point),
    /// That press is let go.
    TabDragEnd,
    /// Where the content is drawn, read once a tab is dragged: let go over it, the tab
    /// splits the tab shown.
    TabDropArea(Option<Rectangle>),
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
    /// A character typed that no widget took: the tree's type-ahead while it has the
    /// keyboard, else the focused list's of the Files tab shown.
    TypeAhead(String),
    /// The window was resized to this size.
    WindowResized(iced::Size),
    /// The handle between the sidebar and the sessions is pressed.
    SidebarDragStart,
    /// The pointer moved to this x while the handle is held.
    SidebarDragged(f32),
    /// The handle is let go.
    SidebarDragEnd,
    /// A divider of a split tab is dragged to this share: drawn there, not kept yet.
    SplitDragged {
        /// The split tab.
        host: TabId,
        /// The divider, by its number in the split.
        divider: usize,
        /// The share its first side takes.
        ratio: f32,
    },
    /// That divider is let go at this share, or moved by an arrow key: kept.
    SplitReleased {
        /// The split tab.
        host: TabId,
        /// The divider, by its number in the split.
        divider: usize,
        /// The share its first side takes.
        ratio: f32,
    },
    /// Quick Connect's search changed.
    PaletteQuery(String),
    /// Open Quick Connect's result at this place.
    PaletteChoose(usize),
    /// Close Quick Connect.
    PaletteClose,
    /// The text of the search bar over `tab`'s terminal changed.
    FinderQuery {
        /// The tab searched.
        tab: TabId,
        /// What is typed.
        query: String,
    },
    /// Look for the text of the search bar over `tab`'s terminal, that way.
    FinderFind {
        /// The tab searched.
        tab: TabId,
        /// Down or up.
        direction: FindDirection,
    },
    /// Close the search bar over this tab's terminal.
    FinderClose(TabId),
    /// Files are dragged over the window, or no longer.
    FilesHovered(bool),
    /// A file or folder dropped on the window: one of a drop, gathered with the others.
    FileDropped(std::path::PathBuf),
    /// The files dropped on a window together all came: sent on as one drop.
    DropGathered(crate::drop_batch::DropPlace),
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
    /// Ctrl+Alt+Home on a remote desktop: the keyboard back to the window.
    ContentRelease,
    /// Show the Settings page's Gateways tab, as the C# Tunnels page's link.
    ManageGateways,
    /// The external editor typed in the Settings page.
    EditorEdited(String),
    /// Apply the external editor typed.
    EditorApply,
    /// A program's path typed in the Settings page: `PuTTY` or the X server.
    ToolPathEdited(ToolPath, String),
    /// Apply the program's path typed.
    ToolPathApply(ToolPath),
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
    /// Something a tab's own window reported of itself.
    Float(window::Id, FloatEvent),
    /// A message of the session drawn in a tab's own window: it reaches the core as the
    /// main window's would, but resolves nothing through the main window's tab shown.
    InFloating(window::Id, Box<Message>),
    /// A message of a tool's tab, whose state the window holds.
    Tool(TabId, crate::tools::ToolMessage),
    /// What is typed in the filter of the sidebar's Tools tab.
    ToolsFilter(String),
    /// What is typed in the Tools page's search.
    ToolsSearch(String),
    /// Open a tool in a tab and show it, as the C# card and sidebar launch it from either
    /// page: the Sessions page shown, the tab named in the language shown.
    OpenTool(heimdall_app::tools::ToolId),
}

/// The tree's shortcuts that hold Ctrl, as the C# Heimdall's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeShortcut {
    /// Ctrl+E: edit the profile selected, when the tree has the keyboard.
    Edit,
    /// Ctrl+N: a new session.
    New,
    /// Quick Connect, from its button. Ctrl+K is a window shortcut, which a terminal and
    /// a desktop leave to the window.
    QuickConnect,
    /// Ctrl+Z: the last move made by a drop in the tree undone.
    Undo,
    /// Ctrl+B: the sidebar shown or hidden, as the C# one.
    ToggleSidebar,
}

impl fmt::Debug for Message {
    #[expect(clippy::too_many_lines, reason = "one arm per message")]
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
            Self::ContentRelease => f.write_str("ContentRelease"),
            Self::ManageGateways => f.write_str("ManageGateways"),
            Self::TrustedSearch(list, _) => write!(f, "TrustedSearch({list:?}, ..)"),
            Self::HostKeySort(column) => write!(f, "HostKeySort({column:?})"),
            Self::LanguageChosen(language) => write!(f, "LanguageChosen({language:?})"),
            Self::ToggleFullscreen => f.write_str("ToggleFullscreen"),
            Self::WindowOpened(_) => f.write_str("WindowOpened"),
            Self::MainWindowClosed => f.write_str("MainWindowClosed"),
            Self::BringForward => f.write_str("BringForward"),
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
            Self::BulkPasswordField { index, .. } => write!(f, "BulkPasswordField({index}, ..)"),
            Self::FocusBulkPasswordField(index) => write!(f, "FocusBulkPasswordField({index})"),
            Self::SubmitBulkPassword => f.write_str("SubmitBulkPassword"),
            Self::SettingsTab(tab) => write!(f, "SettingsTab({tab:?})"),
            Self::ProfileTab(tab) => write!(f, "ProfileTab({tab:?})"),
            Self::SettingsSearch(typed) => write!(f, "SettingsSearch({typed:?})"),
            Self::FindModifiedSettings => f.write_str("FindModifiedSettings"),
            Self::ResetSetting(row) => write!(f, "ResetSetting({row:?})"),
            Self::GoToSetting(row) => write!(f, "GoToSetting({row:?})"),
            Self::BrowseKeyFile => f.write_str("BrowseKeyFile"),
            Self::Browse(target) => write!(f, "Browse({target:?})"),
            Self::Browsed(target, _) => write!(f, "Browsed({target:?}, ..)"),
            Self::GatewayPassword(_) => f.write_str("GatewayPassword(..)"),
            Self::GatewayPassphrase(_) => f.write_str("GatewayPassphrase(..)"),
            Self::SaveGatewayForm => f.write_str("SaveGatewayForm"),
            Self::TestRouteForm => f.write_str("TestRouteForm"),
            Self::CopyRouteReport => f.write_str("CopyRouteReport"),
            Self::OpenTreeMenu(menu) => write!(f, "OpenTreeMenu({menu:?})"),
            Self::CloseTreeMenu => f.write_str("CloseTreeMenu"),
            Self::MenuChoice(message) => write!(f, "MenuChoice({message:?})"),
            Self::MenuFullscreen(tab) => write!(f, "MenuFullscreen({})", tab.value()),
            Self::SplitPalette { host, axis } => {
                write!(f, "SplitPalette({}, {axis:?})", host.value())
            }
            Self::CopyError(tab) => write!(f, "CopyError({})", tab.value()),
            Self::CopyAnonymousError(tab) => write!(f, "CopyAnonymousError({})", tab.value()),
            Self::Tick => f.write_str("Tick"),
            Self::IdleTick => f.write_str("IdleTick"),
            Self::SystemHighContrast(on) => write!(f, "SystemHighContrast({on})"),
            Self::UserInput => f.write_str("UserInput"),
            Self::Modifiers(modifiers) => write!(f, "Modifiers({modifiers:?})"),
            Self::TreeClick(id) => write!(f, "TreeClick({id})"),
            Self::ContentFocus => f.write_str("ContentFocus"),
            Self::TreeShortcut(shortcut) => write!(f, "TreeShortcut({shortcut:?})"),
            Self::GatewayReassignPicked { missing, to } => {
                write!(f, "GatewayReassignPicked({missing}, {to})")
            }
            Self::EditPath { tab, side } => write!(f, "EditPath({}, {side:?})", tab.value()),
            Self::FileColumns { tab, side, widths } => {
                write!(f, "FileColumns({}, {side:?}, {widths:?})", tab.value())
            }
            Self::FilesHover(spot) => write!(f, "FilesHover({spot:?})"),
            Self::FilesHoverLeft(spot) => write!(f, "FilesHoverLeft({spot:?})"),
            Self::PointerPressed => f.write_str("PointerPressed"),
            Self::CloseKey => f.write_str("CloseKey"),
            Self::CloseKeyFocus(focused) => write!(f, "CloseKeyFocus({focused})"),
            Self::TabHover(tab) => write!(f, "TabHover({})", tab.value()),
            Self::TabHoverLeft(tab) => write!(f, "TabHoverLeft({})", tab.value()),
            Self::TabDragMoved(_) => f.write_str("TabDragMoved"),
            Self::TabDragEnd => f.write_str("TabDragEnd"),
            Self::TabDropArea(area) => write!(f, "TabDropArea({area:?})"),
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
            Self::SplitDragged {
                host,
                divider,
                ratio,
            } => write!(f, "SplitDragged({}, {divider}, {ratio})", host.value()),
            Self::SplitReleased {
                host,
                divider,
                ratio,
            } => write!(f, "SplitReleased({}, {divider}, {ratio})", host.value()),
            Self::PaletteQuery(_) => f.write_str("PaletteQuery(..)"),
            Self::PaletteChoose(index) => write!(f, "PaletteChoose({index})"),
            Self::PaletteClose => f.write_str("PaletteClose"),
            Self::FinderQuery { tab, .. } => write!(f, "FinderQuery({tab:?}, ..)"),
            Self::FinderFind { tab, direction } => write!(f, "FinderFind({tab:?}, {direction:?})"),
            Self::FinderClose(tab) => write!(f, "FinderClose({tab:?})"),
            Self::LogDirectoryEdited(_) => f.write_str("LogDirectoryEdited(..)"),
            Self::EditorEdited(_) => f.write_str("EditorEdited(..)"),
            Self::PresetsEdited(_) => f.write_str("PresetsEdited(..)"),
            Self::OpenWithSystem(_) => f.write_str("OpenWithSystem(..)"),
            Self::NewNote { id, template } => write!(f, "NewNote({id}, {template:?})"),
            Self::EditorApply => f.write_str("EditorApply"),
            Self::ToolPathEdited(path, _) => write!(f, "ToolPathEdited({path:?}, ..)"),
            Self::ToolPathApply(path) => write!(f, "ToolPathApply({path:?})"),
            Self::FilesHovered(over) => write!(f, "FilesHovered({over})"),
            Self::FileDropped(_) => f.write_str("FileDropped(..)"),
            Self::DropGathered(place) => write!(f, "DropGathered({place:?})"),
            Self::LogDirectoryApply => f.write_str("LogDirectoryApply"),
            Self::FontSizeEdited(typed) => write!(f, "FontSizeEdited({typed:?})"),
            Self::FontSizeApply => f.write_str("FontSizeApply"),
            Self::SessionFieldEdited(field, typed) => {
                write!(f, "SessionFieldEdited({field:?}, {typed:?})")
            }
            Self::SessionFieldApply(field) => write!(f, "SessionFieldApply({field:?})"),
            Self::Float(window, event) => write!(f, "Float({window:?}, {event:?})"),
            Self::InFloating(window, message) => write!(f, "InFloating({window:?}, {message:?})"),
            // What is typed in a tool can be anything: only its kind is shown.
            Self::Tool(tab, _) => write!(f, "Tool({}, ..)", tab.value()),
            Self::ToolsFilter(typed) => write!(f, "ToolsFilter({typed:?})"),
            Self::ToolsSearch(typed) => write!(f, "ToolsSearch({typed:?})"),
            Self::OpenTool(tool) => write!(f, "OpenTool({tool:?})"),
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

/// Widget identifier of a tab of the sidebar's "Sessions | Tools".
#[must_use]
pub fn sidebar_tab_id(tab: SidebarTab) -> iced::widget::Id {
    iced::widget::Id::new(match tab {
        SidebarTab::Sessions => "sidebar-tab-sessions",
        SidebarTab::Tools => "sidebar-tab-tools",
    })
}

/// The sidebar's "Sessions | Tools", as the C# segmented switch (`MainWindow.xaml:355-384`):
/// two tabs sharing its width, the one `shown` underlined in the accent.
fn sidebar_tabs<'a>(shown: SidebarTab) -> Element<'a, Message> {
    let tab = |label: String, which: SidebarTab| -> Element<'a, Message> {
        let selected = shown == which;
        let mut label = text(label).size(font_size::BODY);
        if selected {
            label = label.font(styles::SEMIBOLD);
        }
        container(
            button(
                column![
                    container(label)
                        .center_x(Length::Fill)
                        .center_y(SIDEBAR_TAB_HEIGHT - SIDEBAR_TAB_UNDERLINE),
                    container(iced::widget::space())
                        .width(Length::Fill)
                        .height(SIDEBAR_TAB_UNDERLINE)
                        .style(styles::underline(selected)),
                ]
                .width(Length::Fill),
            )
            .padding(0.0)
            .width(Length::Fill)
            .style(styles::sidebar_tab(selected))
            .on_press(Message::App(AppMessage::Tools(
                ToolsMessage::ShowSidebarTab(which),
            ))),
        )
        .id(sidebar_tab_id(which))
        .width(Length::FillPortion(1))
        .into()
    };
    row![
        tab(fl!("ui-sidebar-title"), SidebarTab::Sessions),
        tab(fl!("ui-sidebar-tools-tab"), SidebarTab::Tools),
    ]
    .width(Length::Fill)
    .into()
}

/// The marks a tab carries after its title, on the strip or in its pane's header: a macro
/// recorded from it or typed into it, a bell rung in it while it is not `active`.
fn tab_badges<'a>(tab: &Tab, active: bool) -> Vec<Element<'a, Message>> {
    let mut badges = Vec::new();
    if tab.macro_recording.is_some() {
        badges.push(
            text(fl!("ui-tab-recording-badge"))
                .size(font_size::CAPTION)
                .style(text::danger)
                .into(),
        );
    } else if tab.macro_playing.is_some() {
        badges.push(
            text(fl!("ui-tab-macro-badge"))
                .size(font_size::CAPTION)
                .into(),
        );
    }
    if tab.bell && !active {
        badges.push(
            text(fl!("ui-tab-bell-badge"))
                .size(font_size::CAPTION)
                .into(),
        );
    }
    badges
}

/// The mark of a tab whose output is recorded, as the C#'s; `None` when it is not.
fn transcript_mark<'a>(tab: &Tab) -> Option<Element<'a, Message>> {
    tab.transcript.is_some().then(|| {
        tooltip(
            text(fl!("ui-tab-recording")).size(font_size::CAPTION),
            text(fl!("ui-tab-recording-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    })
}

/// What `tab` connects to, as the C# session header says it after the name: a server's
/// address, or a local shell's program; `None` for the default shell.
fn tab_endpoint(tab: &Tab) -> Option<String> {
    if let TabProfile::Local(shell) = &tab.profile {
        return shell
            .program
            .as_deref()
            .and_then(|program| std::path::Path::new(program).file_name())
            .map(|name| name.to_string_lossy().into_owned());
    }
    tab.profile
        .endpoint()
        .map(|(host, port)| server_text(&target(host, port, tab.profile.username())))
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
    /// SSH auto-reconnect, the file transfers, the trusted host keys and the trusted FTPS
    /// certificates.
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
    /// What is typed into the bulk password dialog: the password, then again.
    bulk_password: [Zeroizing<String>; 2],
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
    /// The tab's own window that menu was opened in, at its cursor; `None` for the main
    /// window.
    menu_window: Option<window::Id>,
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
    /// The Files tabs' column widths, as resized; for this run only, as the C#.
    file_columns: HashMap<TabId, crate::files_view::TabColumns>,
    /// Where the pointer is in a Files tab's panes.
    files_hover: Option<crate::files_drag::Spot>,
    /// The tab's own window the pointer was last over a Files tab's place in; `None` for
    /// the main window.
    files_hover_window: Option<window::Id>,
    /// A press on a Files tab's entry, held: a drag once the pointer moves.
    files_drag: Option<crate::files_drag::FilesDrag>,
    /// The tab's own window that press was in; `None` for the main window.
    files_drag_window: Option<window::Id>,
    /// The tab under the pointer.
    tab_hover: Option<TabId>,
    /// A press on a tab, a drag once the pointer moves.
    tab_drag: Option<crate::tab_drag::TabDrag>,
    /// Where the content was drawn when that drag started.
    tab_drop_area: Option<Rectangle>,
    /// The tab detached by a drag let go out of the window, and where the pointer was on
    /// the screens, in physical pixels: where its window opens.
    detach_place: Option<(FloatId, (f64, f64))>,
    /// The computer kept from sleeping while a session is open.
    sleep_guard: crate::sleep_guard::SleepGuard,
    /// A press in the tree, held: a drag once the pointer moves.
    tree_drag: Option<crate::tree_drag::TreeDrag>,
    /// The sidebar is hidden, Ctrl+B having hidden it.
    sidebar_hidden: bool,
    /// The theme Windows imposes over the one chosen: high contrast, while it is on.
    forced_theme: Option<AppTheme>,
    /// Where the window's state is kept, and how it was left; none in tests.
    window_memory: Option<(PathBuf, heimdall_core::window_state::WindowState)>,
    /// The main window, named once it is asked to open; none in tests, which open none.
    main_window: Option<window::Id>,
    /// The configuration folder this instance owns, where a later launch asks it to come
    /// forward; none in tests, and when started unguarded.
    instance_dir: Option<PathBuf>,
    /// The tabs' own windows, by the window's identifier.
    floating: std::collections::BTreeMap<window::Id, FloatingWindow>,
    /// The window's size, as last resized out of full screen.
    window_size: Option<iced::Size>,
    /// The window's drawn area, as last reported, in full screen too: what a tab dragged
    /// out of it is let go beyond to detach.
    window_extent: Option<iced::Size>,
    /// This computer's screens while an RDP profile form is open, as its multi-monitor
    /// picker offers them: listed when the form opens and again when the main window is
    /// rescaled or resized, never while it is drawn; none while no such form is open.
    monitors: Option<Vec<crate::rdp_options::Monitor>>,
    /// The sidebar's width, as dragged.
    sidebar_width: f32,
    /// The handle between the sidebar and the sessions is held.
    sidebar_drag: bool,
    /// A divider of a split tab being dragged: the tab, the divider's number, the share it
    /// is drawn at until let go.
    split_drag: Option<(TabId, usize, f32)>,
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
    /// The programs' paths typed in the Settings page, until applied, by
    /// [`ToolPath::index`].
    tool_paths_typed: [Option<String>; ToolPath::COUNT],
    /// The Settings page's box of resolution presets.
    presets: crate::presets_editor::PresetsEditor,
    /// The terminals' font size as typed in the Settings page, until applied.
    font_size_typed: Option<String>,
    /// The numbers of the session card as typed in the Settings page, until applied, by
    /// [`SessionField::index`].
    session_typed: [Option<String>; SessionField::COUNT],
    /// The search typed over the trusted SSH host keys.
    host_key_search: String,
    /// How the trusted SSH host keys are sorted, while the application runs.
    host_key_sort: HostKeySort,
    /// The Settings tab shown, kept while the application runs.
    settings_tab: SettingsTab,
    /// The profile form's tab shown, General each time the form opens.
    profile_tab: ProfileTab,
    /// Whether a save of the open profile form was refused: from then on, as the C# server
    /// dialog, each tab's header counts the fields left to fix on it.
    profile_refused: bool,
    /// What the Settings page's search holds: while it holds a word, the rows it finds are
    /// shown in place of the tab.
    settings_search: String,
    /// The row a "Go to setting" of the security overview showed, outlined until another
    /// tab or a search is chosen.
    settings_highlight: Option<SettingRow>,
    /// The search typed over the trusted RDP certificates.
    certificate_search: String,
    /// The search typed over the trusted FTPS certificates.
    ftps_certificate_search: String,
    /// The search typed over the trusted VNC certificates.
    vnc_certificate_search: String,
    /// The window's own last key, click or wheel turn: the idle time where the computer's
    /// cannot be read.
    last_input: std::time::Instant,
    /// Files are dragged over the window.
    files_hovered: bool,
    /// The files of the drops still gathered, by the window they were dropped on.
    drops: crate::drop_batch::DropBatches,
    /// A field that gets the keyboard once this update is drawn: Quick Connect's or the
    /// search bar's, just opened.
    focus_next: Option<iced::widget::Id>,
    /// Desktops shown otherwise than their protocol's default: fitted or matched.
    desktop_fit: HashMap<TabId, (bool, Option<(u16, u16)>)>,
    /// What the tree's search holds: the profiles it finds are shown.
    search: String,
    /// What the tool tabs hold, by tab.
    tools: crate::tools::ToolPanes,
    /// What the filter of the sidebar's Tools tab holds.
    tools_filter: String,
    /// What the Tools page's search holds.
    tools_search: String,
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
    /// The built-in tools, as the C# Tools page.
    Tools,
    /// The settings, over the tab shown when they were opened: showing another tab leaves
    /// them.
    Settings {
        /// That tab.
        over: Option<TabId>,
    },
}

/// A page of the window's navigation, as the C# toolbar's tabs. The C# Scheduled page comes
/// with what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// The sessions: the tree beside them.
    Sessions,
    /// Every tunnel.
    Tunnels,
    /// The built-in tools.
    Tools,
    /// The settings.
    Settings,
    /// About Heimdall.
    About,
}

impl Destination {
    /// Every page, in the C# order.
    pub const ALL: [Self; 5] = [
        Self::Sessions,
        Self::Tunnels,
        Self::Tools,
        Self::Settings,
        Self::About,
    ];

    fn label(self) -> String {
        match self {
            Self::Sessions => fl!("ui-nav-sessions"),
            Self::Tunnels => fl!("ui-nav-tunnels"),
            Self::Tools => fl!("ui-nav-tools"),
            Self::Settings => fl!("ui-nav-settings"),
            Self::About => fl!("ui-nav-about"),
        }
    }
}

/// What an RDP session shares, as the C# session bar's indicators: the clipboard, the
/// drives, the sound played here; each says what it is when pointed at.
fn redirection_badges<'a>(profile: &RdpProfile) -> Vec<Element<'a, Message>> {
    [
        (
            profile.redirect_clipboard,
            fl!("ui-desktop-shares-clipboard"),
            fl!("ui-desktop-shares-clipboard-tooltip"),
        ),
        (
            profile.redirect_drives,
            fl!("ui-desktop-shares-drives"),
            fl!("ui-desktop-shares-drives-tooltip"),
        ),
        (
            profile.options.audio == heimdall_core::profile::AudioPlayback::Local,
            fl!("ui-desktop-shares-audio"),
            fl!("ui-desktop-shares-audio-tooltip"),
        ),
    ]
    .into_iter()
    .filter(|(on, _, _)| *on)
    .map(|(_, label, tip)| {
        tooltip(
            text(label).size(font_size::CAPTION).style(text::secondary),
            text(tip).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    })
    .collect()
}

/// The application's name, at the head of the navigation: a name, not translated.
const APP_NAME: &str = "Heimdall";

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
    /// The bulk password dialog's first field.
    BulkPassword,
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
        // Windows high contrast, as it is at start; looked at again while the window runs.
        shell.forced_theme =
            heimdall_app::high_contrast::system_on().then_some(AppTheme::HighContrast);
        // The tree as it was left.
        shell
            .app
            .restore_tree(&left.folded, left.selected.as_deref().map(ProfileId::new));
        shell.window_memory = Some((memory, left));
        // The language chosen, once the settings are read; else the desktop's, set at start.
        if let Some(language) = shell.app.settings().language {
            crate::i18n::apply(Some(language));
        }
        // Transcripts past their retention, removed beside the start, as the C# does.
        shell.app.prune_transcripts();
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
            bulk_password: Default::default(),
            sudo_password: Zeroizing::default(),
            profile_passphrase: Zeroizing::default(),
            provider_unlock: Zeroizing::default(),
            gateway_password: Zeroizing::default(),
            gateway_passphrase: Zeroizing::default(),
            cursor: CursorSpot::default(),
            menu: None,
            menu_window: None,
            page: Page::Tab,
            fullscreen: false,
            density: 1.0,
            modifiers: keyboard::Modifiers::empty(),
            tree_focused: false,
            tree_focus: None,
            language_shown,
            gateway_reassign: std::collections::BTreeMap::new(),
            path_editing: None,
            file_columns: HashMap::new(),
            files_hover: None,
            files_hover_window: None,
            files_drag: None,
            files_drag_window: None,
            tab_hover: None,
            tab_drag: None,
            tab_drop_area: None,
            detach_place: None,
            sleep_guard: crate::sleep_guard::SleepGuard::new(),
            tree_drag: None,
            sidebar_hidden: false,
            forced_theme: None,
            window_memory: None,
            main_window: None,
            instance_dir: None,
            floating: std::collections::BTreeMap::new(),
            window_size: None,
            window_extent: None,
            monitors: None,
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_drag: false,
            split_drag: None,
            type_ahead: (String::new(), None),
            palette: None,
            finder: None,
            focus_next: None,
            log_directory: None,
            editor_typed: None,
            tool_paths_typed: Default::default(),
            presets,
            font_size_typed: None,
            session_typed: Default::default(),
            host_key_search: String::new(),
            host_key_sort: HostKeySort::default(),
            settings_tab: SettingsTab::default(),
            profile_tab: ProfileTab::default(),
            profile_refused: false,
            settings_search: String::new(),
            settings_highlight: None,
            certificate_search: String::new(),
            ftps_certificate_search: String::new(),
            vnc_certificate_search: String::new(),
            last_input: std::time::Instant::now(),
            files_hovered: false,
            drops: crate::drop_batch::DropBatches::default(),
            desktop_fit: HashMap::new(),
            search: String::new(),
            tools: crate::tools::ToolPanes::default(),
            tools_filter: String::new(),
            tools_search: String::new(),
        }
        .with_tool_panes()
    }

    /// The tool tabs' states made for the tabs the core already holds.
    fn with_tool_panes(mut self) -> Self {
        self.tools.sync(&self.app);
        self
    }

    /// The field Ctrl+F gives the keyboard to: the Settings page's search on that page, as
    /// the C# Settings tab's; a Files tab's filter in its lists, as the C# file browser's;
    /// else the tree's search.
    #[must_use]
    pub fn search_field(&self) -> iced::widget::Id {
        if self.settings_shown() {
            return settings_search_field_id();
        }
        match self.shown_files_side() {
            Some((tab, side)) => files_view::field_id(tab, side, files_view::PaneField::Filter),
            None => search_field_id(),
        }
    }

    /// Whether the settings are shown.
    #[must_use]
    pub fn settings_shown(&self) -> bool {
        self.page
            == Page::Settings {
                over: self.shown_tab_id(),
            }
    }

    /// The tab of the strip shown, whichever pane of its split has the keyboard.
    fn shown_tab_id(&self) -> Option<TabId> {
        self.app.shown_tab().map(|tab| tab.id)
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

    /// Takes `monitors` as this computer's screens for the RDP profile form open, in place
    /// of those listed, until the window is next rescaled or resized or the form closes:
    /// what a test gives the multi-monitor picker.
    pub fn set_monitors(&mut self, monitors: Vec<crate::rdp_options::Monitor>) {
        self.monitors = Some(monitors);
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

    /// The title of the window `window`: a tab's own window's as the C# names it, the main
    /// window's as [`Self::title`], the application's name for any other.
    #[must_use]
    pub fn window_title(&self, window: window::Id) -> String {
        if let Some(tab) = self.floating_tab_of(window) {
            return fl!("ui-window-title-detached", tab = tab.display_title());
        }
        if from_main(self.main_window, window) {
            self.title()
        } else {
            fl!("ui-window-title")
        }
    }

    /// What starts with the application: Credential Guard checked in the background when the
    /// settings require it, so that the first embedded RDP session does not wait for it; and
    /// the user's OpenSSH `known_hosts` imported in the background when chosen, as the C#
    /// does at startup, the folders' permissions already set.
    pub fn start_tasks(&mut self) -> Task<Message> {
        let mut effects = self.app.warm_credential_guard();
        effects.extend(self.app.sync_known_hosts_at_startup());
        let tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        Task::batch(tasks)
    }

    /// Names the main window, asked to open: its events are the only ones taken, and the
    /// dialogs, the screenshot and the place kept at exit are its own.
    pub fn set_main_window(&mut self, window: window::Id) {
        self.main_window = Some(window);
    }

    /// Answers the later launches' requests to come forward, left in `dir`, the
    /// configuration folder this instance owns.
    pub fn watch_instance(&mut self, dir: PathBuf) {
        self.instance_dir = Some(dir);
    }

    /// Draws the window `window`: a tab's own window with its session, the main window as
    /// [`Self::view`], nothing in any other.
    #[must_use]
    pub fn window_view(&self, window: window::Id) -> Element<'_, Message> {
        if let Some(floating) = self.floating.get(&window) {
            self.floating_view(window, floating.key)
        } else if from_main(self.main_window, window) {
            self.view()
        } else {
            iced::widget::space().into()
        }
    }

    /// The window `tab` is detached to, once it is asked to open.
    #[must_use]
    pub fn floating_window(&self, tab: TabId) -> Option<window::Id> {
        self.app
            .floating_of(tab)
            .and_then(|key| self.window_of(key))
    }

    /// The window the application calls `key`.
    fn window_of(&self, key: FloatId) -> Option<window::Id> {
        self.floating
            .iter()
            .find(|(_, floating)| floating.key == key)
            .map(|(window, _)| *window)
    }

    /// The tab shown in `window`, a tab's own window.
    fn floating_tab_of(&self, window: window::Id) -> Option<&Tab> {
        self.floating
            .get(&window)
            .and_then(|floating| self.app.floating_tab(floating.key))
    }

    /// The window a question about `tab` is held by: its own window when it is detached,
    /// else the main one.
    fn owner_of(&self, tab: TabId) -> Option<window::Id> {
        self.floating_window(tab).or(self.main_window)
    }

    /// Physical pixels per logical one on the screen `tab` is drawn on.
    fn density_of(&self, tab: TabId) -> f32 {
        self.floating_window(tab)
            .and_then(|window| self.floating.get(&window))
            .map_or(self.density, |floating| floating.scale)
    }

    /// A tab's own window: its header above its session, drawn as its tab draws it, every
    /// message of the session marked as the window's; over a Files tab, the files dragged
    /// from Explorer said and its menu open; behind the lock, a veil.
    fn floating_view(&self, window: window::Id, key: FloatId) -> Element<'_, Message> {
        let Some(tab) = self.app.floating_tab(key).filter(|_| !self.gated()) else {
            return crate::floating_view::veil();
        };
        let Some(floating) = self.floating.get(&window) else {
            return crate::floating_view::veil();
        };
        let route = self.app.tab_route(tab);
        let heading = crate::floating_view::Header {
            key,
            state: SessionState::of(tab),
            kind: self.app.tab_kind(tab).label().to_owned(),
            title: tab.display_title().to_owned(),
            route: (!route.is_empty()).then(|| {
                let names = route
                    .iter()
                    .map(|name| server_text(name))
                    .collect::<Vec<_>>()
                    .join(&fl!("ui-route-test-separator"));
                fl!("ui-connect-via", route = names)
            }),
        };
        let body = self
            .tab_page(tab, true)
            .map(move |message| Message::InFloating(window, Box::new(message)));
        let mut layers = stack![crate::floating_view::view(heading, body)];
        if floating.hovered && takes_drops(tab) {
            layers = layers.push(drop_layer());
        }
        if let Some(menu) = self.floating_menu(tab.id) {
            layers = layers
                .push(menu.map(move |message| Message::InFloating(window, Box::new(message))));
        }
        CursorTracker::new(layers, floating.cursor.clone()).into()
    }

    /// Theme: the one the Settings page chose, tinted with its accent; high contrast while
    /// Windows has it on. The terminals keep their own colour scheme, apart from it as in
    /// the C#.
    #[must_use]
    pub fn theme(&self) -> Theme {
        crate::themes::theme(self.theme_shown(), self.app.settings().accent)
    }

    /// The theme drawn: Windows high contrast's while it is on, else the one chosen.
    #[must_use]
    pub fn theme_shown(&self) -> AppTheme {
        self.forced_theme.unwrap_or(self.app.settings().theme)
    }

    /// The colours the integrated editor highlights code with, light on a light theme.
    fn editor_syntax(&self) -> iced::highlighter::Theme {
        crate::themes::syntax(self.theme_shown())
    }

    /// The application's own timers: the sessions kept alive, the servers checked, the
    /// look for a newer release.
    fn background_ticks(&self) -> Vec<Subscription<Message>> {
        let mut ticks = Vec::new();
        if let Some(interval) = self.app.tmout_reset_interval() {
            ticks.push(
                iced::time::every(interval).map(|_| Message::App(AppMessage::TmoutResetTick)),
            );
        }
        // Every server checked in the background, as the C# session health monitor.
        if let Some(interval) = self.app.reachability_interval() {
            ticks.push(
                iced::time::every(interval).map(|_| Message::App(AppMessage::ReachabilityTick)),
            );
        }
        // The look for a newer release, a while after start then hourly, as the C# banner's.
        if let Some(interval) = self.app.update_tick_interval() {
            ticks.push(
                iced::time::every(interval)
                    .map(|_| Message::App(AppMessage::Update(heimdall_app::UpdateMessage::Tick))),
            );
        }
        // The servers whose health panel is shown, asked as the C# asks them.
        if self.app.polls_health() {
            ticks.push(
                iced::time::every(heimdall_app::server_health::HEALTH_INTERVAL)
                    .map(|_| Message::App(AppMessage::HealthTick)),
            );
        }
        // The Citrix tabs' clients, looked at as the C# health check looks at its session.
        if self.app.polls_citrix() {
            ticks.push(
                iced::time::every(heimdall_app::citrix_session::HEALTH_INTERVAL)
                    .map(|_| Message::App(AppMessage::CitrixTick)),
            );
        }
        if let Some(interval) = self.app.anti_idle_interval() {
            ticks.push(iced::time::every(interval).map(|_| Message::App(AppMessage::AntiIdleTick)));
        }
        // The RDP desktops settling after connecting, while one does: the end of the wait
        // and the session bar's countdown.
        if let Some(interval) = self.app.stabilization_interval() {
            ticks.push(
                iced::time::every(interval)
                    .map(|at| Message::App(AppMessage::StabilizationTick(at))),
            );
        }
        // Windows high contrast turned on or off while the window is open.
        if heimdall_app::high_contrast::WATCHED {
            ticks.push(
                iced::time::every(heimdall_app::high_contrast::POLL)
                    .map(|_| Message::SystemHighContrast(heimdall_app::high_contrast::system_on())),
            );
        }
        ticks
    }

    /// Window events and shortcuts.
    pub fn subscription(&self) -> Subscription<Message> {
        // The main window's events only: another window's keys, moves and closing are not
        // the main window's.
        let main = self.main_window;
        let events = Subscription::batch([
            in_main_window(window_tagged!(window_event), main),
            // The screen's density is known once the window is open.
            in_main_window(
                window::open_events().map(|id| (id, Message::WindowOpened(id))),
                main,
            ),
            // A daemon outlives its windows: the main window closed, the application ends.
            in_main_window(
                window::close_events().map(|id| (id, Message::MainWindowClosed)),
                main,
            ),
            // The tabs' own windows: their close, focus and screen, each marked as its own.
            window_tagged!(crate::floating_view::window_event)
                .with(main)
                .filter_map(crate::floating_view::from_floating),
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
        // A later launch handing over to this instance, as the C# activation event.
        if let Some(dir) = &self.instance_dir {
            subscriptions.push(crate::single_instance::requests(dir.clone()));
        }
        // A drag of a Files tab's entries, followed in the window it started in.
        if self.files_drag.is_some() {
            subscriptions.push(in_main_window(
                window_tagged!(crate::files_drag::drag_event),
                self.files_drag_window.or(main),
            ));
        }
        if self.tab_drag.is_some() {
            subscriptions.push(in_main_window(
                window_tagged!(crate::tab_drag::drag_event),
                main,
            ));
        }
        if self.tree_drag.is_some() {
            subscriptions.push(in_main_window(
                window_tagged!(crate::tree_drag::drag_event),
                main,
            ));
        }
        if self.sidebar_drag {
            subscriptions.push(in_main_window(window_tagged!(sidebar_drag_event), main));
        }
        if locked_out
            || self.app.tabs.iter().any(|tab| tab.retry.is_some())
            || self.app.undo_offer().is_some()
        {
            subscriptions.push(iced::time::every(COUNTDOWN_TICK).map(|_| Message::Tick));
        }
        subscriptions.extend(self.background_ticks());
        // The idle time measured while the workspace can lock by itself, as the C# idle
        // timer. Where it is not the computer's, the window's own input is always counted:
        // counted only once a threshold is set, the time before would count as idle, and
        // the workspace would lock as soon as one is.
        if self.app.watches_idle() {
            subscriptions
                .push(iced::time::every(heimdall_app::IDLE_POLL).map(|_| Message::IdleTick));
        }
        if !crate::idle::SYSTEM_WIDE {
            subscriptions.push(event::listen_with(crate::idle::input_event));
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

    /// Applies a message, then keeps the computer awake while a session is connected, as
    /// the C# `SleepPrevention` does when the setting is on.
    pub fn step(&mut self, message: Message) -> Task<Message> {
        let task = self.update(message);
        let task = Task::batch([task, self.sync_floating()]);
        let awake = self.app.settings().prevent_sleep
            && self
                .app
                .tabs
                .iter()
                .any(|tab| tab.phase == Phase::Connected);
        self.sleep_guard.hold(awake);
        task
    }

    /// Whether the computer is kept from sleeping now.
    #[must_use]
    pub fn keeps_awake(&self) -> bool {
        self.sleep_guard.held()
    }

    /// The tabs' own windows as the core has them: a window whose tab went back or is gone
    /// closes, a tab detached with no window gets one; the search bar of a tab detached
    /// closes, the main window's keys no longer reaching it, and so does a window's bar
    /// over a tab it no longer shows, its session opened again as another.
    fn sync_floating(&mut self) -> Task<Message> {
        if self
            .finder
            .as_ref()
            .is_some_and(|finder| self.app.is_floating(finder.tab))
        {
            self.finder = None;
        }
        for floating in self.floating.values_mut() {
            let shown = self.app.floating_tab(floating.key).map(|tab| tab.id);
            if floating
                .finder
                .as_ref()
                .is_some_and(|finder| Some(finder.tab) != shown)
            {
                floating.finder = None;
            }
        }
        let gone: Vec<FloatId> = self
            .floating
            .values()
            .map(|floating| floating.key)
            .filter(|key| self.app.floating_tab(*key).is_none())
            .collect();
        let missing: Vec<FloatId> = self
            .app
            .floating()
            .iter()
            .map(|floating| floating.key)
            .filter(|key| self.window_of(*key).is_none())
            .collect();
        let mut tasks: Vec<Task<Message>> = gone
            .into_iter()
            .map(|key| self.close_floating(key))
            .collect();
        tasks.extend(missing.into_iter().map(|key| self.open_floating(key)));
        Task::batch(tasks)
    }

    /// Opens the window of the tab detached as `key`, at the C# size, and gives it the
    /// focus; one already open is focused. Centred, as the C#'s, unless the tab was dragged
    /// out of the main window: then opened hidden, moved for the pointer to be on its title
    /// bar, then shown.
    fn open_floating(&mut self, key: FloatId) -> Task<Message> {
        if let Some(window) = self.window_of(key) {
            return focus_window(window);
        }
        let place = self
            .detach_place
            .take()
            .filter(|(placed, _)| *placed == key)
            .map(|(_, pointer)| pointer);
        let settings = window::Settings {
            visible: place.is_none(),
            ..crate::floating_view::settings()
        };
        let (window, opened) = window::open(settings);
        self.floating
            .insert(window, FloatingWindow::new(key, self.density));
        opened.then(move |window| {
            let shown = place.map_or_else(Task::none, |pointer| {
                crate::screens::show_near_pointer(
                    window,
                    pointer,
                    crate::floating_view::WINDOW_SIZE,
                    crate::tab_drag::DETACH_GRAB,
                )
            });
            Task::batch([
                shown.chain(window::gain_focus(window)),
                window::scale_factor(window)
                    .map(move |scale| Message::Float(window, FloatEvent::Rescaled(scale))),
            ])
        })
    }

    /// Closes the window of the tab detached as `key`: its tab went back to the strip, or is
    /// gone. What was opened or pointed at in it goes with it, its menu, the place under its
    /// pointer and a drag started there, so nothing of it shows in the main window at its
    /// cursor's place.
    fn close_floating(&mut self, key: FloatId) -> Task<Message> {
        let Some(window) = self.window_of(key) else {
            return Task::none();
        };
        self.floating.remove(&window);
        if self.menu_window == Some(window) {
            self.menu = None;
            self.menu_window = None;
        }
        if self.files_hover_window == Some(window) {
            self.files_hover = None;
            self.files_hover_window = None;
        }
        if self.files_drag_window == Some(window) {
            self.files_drag = None;
            self.files_drag_window = None;
        }
        window::close(window)
    }

    /// What a tab's own window reported of itself: its close button asks the core, its
    /// focus is its session's, its screen its desktop's; the keys, drops and presses of a
    /// Files tab are that tab's.
    fn float_event(&mut self, window: window::Id, event: FloatEvent) -> Task<Message> {
        let Some(key) = self.floating.get(&window).map(|floating| floating.key) else {
            return Task::none();
        };
        let message = match event {
            // Behind the lock, the tab goes back to the strip without a question, which
            // would take the lock screen's place.
            FloatEvent::CloseRequested if self.gated() => FloatMessage::Reattach(key),
            FloatEvent::CloseRequested => FloatMessage::CloseRequested(key),
            FloatEvent::Focused(focused) => FloatMessage::Focused { key, focused },
            FloatEvent::Rescaled(scale) => {
                if let Some(floating) = self.floating.get_mut(&window) {
                    floating.scale = scale;
                }
                return Task::none();
            }
            FloatEvent::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                return Task::none();
            }
            FloatEvent::TerminalFind => return self.toggle_floating_finder(window),
            // The status bar is the main window's: copied from there.
            FloatEvent::CopyStatus => {
                return self.update(Message::Shortcut(WindowShortcut::CopyStatus));
            }
            // Quick Connect is the main window's, brought forward for it.
            FloatEvent::QuickConnect => {
                let task = self.update(Message::Shortcut(WindowShortcut::QuickConnect));
                if self.palette.is_none() {
                    return task;
                }
                let main = main_window_task(self.main_window)
                    .then(|main| main.map_or_else(Task::none, focus_window));
                return Task::batch([task, main]);
            }
            FloatEvent::Escape if self.close_floating_finder(window) => return Task::none(),
            event @ (FloatEvent::FilesKey(_)
            | FloatEvent::FindKey
            | FloatEvent::Escape
            | FloatEvent::PointerPressed
            | FloatEvent::FilesHovered(_)
            | FloatEvent::FileDropped(_)
            | FloatEvent::TypeAhead(_)) => return self.floating_files_event(window, event),
        };
        let effects = self.app.update(AppMessage::Float(message));
        Task::batch(effects.into_iter().map(|effect| self.run(effect)))
    }

    /// A message of the session drawn in a tab's own window. Behind the lock it is dropped;
    /// a zoom, which names no tab, is made that tab's, and so is the close of a menu drawn
    /// there; anything else passes only when
    /// [`crate::floating_view::floating_message_allowed`] lets it, naming that tab. A menu
    /// of its Files pane opens where the pointer is in that window.
    fn in_floating(&mut self, window: window::Id, message: Message) -> Task<Message> {
        let Some(tab) = self.floating_tab_of(window) else {
            return Task::none();
        };
        if self.gated() {
            return Task::none();
        }
        if let Message::Shortcut(WindowShortcut::Zoom(zoom)) = message {
            let tab = tab.id;
            self.zoom(tab, zoom);
            return Task::none();
        }
        if matches!(message, Message::CloseTreeMenu) {
            let tab = tab.id;
            self.close_floating_menu(tab);
            return Task::none();
        }
        if !crate::floating_view::floating_message_allowed(&message, tab) {
            log::debug!("dropped from a detached window: {message:?}");
            return Task::none();
        }
        if let Message::OpenTreeMenu(menu) = message {
            let at = self
                .floating
                .get(&window)
                .map_or_else(|| self.cursor.get(), |floating| floating.cursor.get());
            self.open_tree_menu_at(menu, at, Some(window));
            return Task::none();
        }
        let hover = matches!(message, Message::FilesHover(_));
        let task = self.apply_floating(message);
        if hover {
            self.files_hover_window = Some(window);
        }
        task
    }

    /// Applies `message`, sent from a tab's own window. A dialog it opens is the main
    /// window's, as the C# dialogs are: that window is brought forward for it.
    fn apply_floating(&mut self, message: Message) -> Task<Message> {
        let asked = self.app.dialog.is_some();
        let task = self.update(message);
        if !asked && self.app.dialog.is_some() {
            let main = main_window_task(self.main_window)
                .then(|main| main.map_or_else(Task::none, focus_window));
            return Task::batch([task, main]);
        }
        task
    }

    /// Ctrl+W, as the C#: the session shown closes when nothing that takes text has the
    /// keyboard, which is asked first; with a dialog, a menu, Quick Connect, the search bar
    /// or a path bar open, the key is theirs.
    fn close_key(&mut self, message: &Message) -> Task<Message> {
        match *message {
            Message::CloseKey => {
                let open = self.app.dialog.is_some()
                    || self.palette.is_some()
                    || self.finder.is_some()
                    || self.menu.is_some()
                    || self.main_path_editing().is_some()
                    || self.app.active.is_none();
                if open {
                    Task::none()
                } else {
                    iced::advanced::widget::operate(crate::search_keys::AnyFocused::default())
                        .map(Message::CloseKeyFocus)
                }
            }
            Message::CloseKeyFocus(false) => {
                self.update(Message::Shortcut(WindowShortcut::CloseTab))
            }
            _ => Task::none(),
        }
    }

    /// Applies a message.
    #[expect(clippy::too_many_lines, reason = "one arm per family of messages")]
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // A tab's own window's: answered apart, never through the main window's tab shown.
        let message = match message {
            Message::Float(window, event) => return self.float_event(window, event),
            Message::InFloating(window, message) => return self.in_floating(window, *message),
            message => message,
        };
        // A click on another row of the tree ends a rename made in place, kept, as the C#
        // editor losing the focus.
        if self.inline_rename().is_some()
            && matches!(
                message,
                Message::TreeClick(_) | Message::App(AppMessage::ToggleFolder(_))
            )
        {
            let _ = self.app.update(AppMessage::ConfirmDialog);
        }
        // Behind the lock screen, the window's keys do nothing; its sessions go on. Nothing
        // else of the window is drawn to be clicked.
        if self.gated() && matches!(message, Message::Shortcut(_) | Message::FilesKey(_)) {
            return Task::none();
        }
        // Escape, a tab dragged, gives the drag up first, taken by a widget or not.
        if matches!(
            message,
            Message::EscapeUntaken | Message::DialogKey { confirm: false }
        ) && self.cancel_tab_drag()
        {
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
        ) || (matches!(message, Message::TypeAhead(_)) && !self.tree_focused);
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
            | Message::ProfileTab(_)
            | Message::ProfilePassword(_)
            | Message::BulkPasswordField { .. }
            | Message::FocusBulkPasswordField(_)
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
            Message::EscapeUntaken | Message::Float(..) | Message::InFloating(..) => Vec::new(),
            Message::FilesKey(key) => self.files_key(key),
            Message::TabKey { backward } => {
                if self.tab_moves_focus() {
                    // Through the dialog's fields alone, never those under its veil; else
                    // the main window's: an operation reaches every window, and a tab's own
                    // window has fields of its own.
                    use iced::advanced::widget::operation::{focusable, scope};
                    let area = if self.app.dialog.is_some() && self.inline_rename().is_none() {
                        dialog_area_id()
                    } else {
                        main_area_id()
                    };
                    return if backward {
                        iced::advanced::widget::operate(scope(area, focusable::focus_previous()))
                    } else {
                        iced::advanced::widget::operate(scope(area, focusable::focus_next()))
                    };
                }
                self.files_key(FilesKey::SwitchPane)
            }
            Message::LockKey => self.closing_menu(AppMessage::LockVault),
            Message::IdleTick => self
                .app
                .update(AppMessage::Idle(crate::idle::idle_time(self.last_input))),
            Message::SystemHighContrast(on) => {
                self.forced_theme = on.then_some(AppTheme::HighContrast);
                Vec::new()
            }
            Message::UserInput => {
                self.last_input = std::time::Instant::now();
                Vec::new()
            }
            Message::MainWindowClosed => return iced::exit(),
            Message::BringForward => return self.main_window.map_or_else(Task::none, focus_window),
            message @ (Message::DesktopFit { .. }
            | Message::ToggleFullscreen
            | Message::WindowOpened(_)
            | Message::Rescaled(_)
            | Message::ShowSettings
            | Message::Navigate(_)
            | Message::ManageGateways
            | Message::TrustedSearch(..)
            | Message::HostKeySort(_)
            | Message::LanguageChosen(_)
            | Message::Modifiers(_)
            | Message::Tick) => return self.view_message(&message),
            message @ (Message::CloseKey | Message::CloseKeyFocus(_)) => {
                return self.close_key(&message);
            }
            // Under a dialog, the tree is not there to search.
            Message::FocusSearch if self.app.dialog.is_some() => return Task::none(),
            Message::FocusSearch => {
                let field = self.search_field();
                return operation::focus(field.clone()).chain(operation::select_all(field));
            }
            message @ (Message::SearchSubmit | Message::SearchDown) => {
                return self.search_key(&message);
            }
            message @ (Message::SettingsSearch(_)
            | Message::FindModifiedSettings
            | Message::GoToSetting(_)) => return self.settings_search_message(message),
            Message::ResetSetting(row) => self.reset_setting(row),
            Message::SubmitVault => self.submit_vault(),
            Message::SubmitPin => self.submit_pin(),
            Message::RemovePin => self.remove_pin(),
            Message::SaveProviderUnlock => self.save_provider_unlock(),
            Message::SaveProfileForm => self.save_profile_form(),
            Message::SubmitBulkPassword => self.submit_bulk_password(),
            Message::SaveGatewayForm => self.save_gateway_form(),
            Message::TestRouteForm => self.test_route_form(),
            message @ (Message::OpenTreeMenu(_)
            | Message::CloseTreeMenu
            | Message::ResetTreeFilters) => return self.tree_menu_message(message),
            Message::MenuChoice(message) => self.closing_menu(message),
            Message::MenuFullscreen(tab) => return self.menu_fullscreen(tab),
            Message::SplitPalette { host, axis } => {
                self.open_palette(Some((host, axis)));
                Vec::new()
            }
            Message::BrowseKeyFile => return pick_key_file(self.main_window),
            Message::Browse(target) => return self.browse(target),
            Message::Browsed(target, path) => self.browsed(target, path),
            Message::CopyError(tab) => return self.copy_error(tab),
            Message::CopyAnonymousError(tab) => {
                return self
                    .anonymous_report(tab, std::time::SystemTime::now())
                    .map_or_else(Task::none, iced::clipboard::write);
            }
            Message::EditPath { tab, side } => {
                self.edit_path(tab, side);
                Vec::new()
            }
            Message::FileColumns { tab, side, widths } => {
                // Tabs closed since leave their widths behind no longer.
                self.file_columns
                    .retain(|id, _| self.app.tab(*id).is_some());
                let columns = self.file_columns.entry(tab).or_default();
                *columns = columns.with(side, widths);
                Vec::new()
            }
            message @ (Message::TreeClick(_)
            | Message::ContentFocus
            | Message::ContentRelease
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
            Message::TabDragMoved(at) => return self.tab_drag_moved(at),
            message @ (Message::TabHover(_)
            | Message::TabHoverLeft(_)
            | Message::TabDropArea(_)
            | Message::TabDragEnd) => self.tab_drag_message(&message),
            message @ (Message::SplitDragged { .. } | Message::SplitReleased { .. }) => {
                self.split_drag_message(&message)
            }
            message @ (Message::FilesHover(_)
            | Message::FilesHoverLeft(_)
            | Message::PointerPressed
            | Message::FilesDragMoved(_)
            | Message::FilesDragEnd) => self.files_drag_message(&message),
            message @ (Message::PaletteQuery(_)
            | Message::PaletteChoose(_)
            | Message::PaletteClose) => self.palette_message(message),
            message @ (Message::FinderQuery { .. }
            | Message::FinderFind { .. }
            | Message::FinderClose(_)) => self.finder_message(message),
            message @ (Message::LogDirectoryEdited(_)
            | Message::LogDirectoryApply
            | Message::EditorEdited(_)
            | Message::PresetsEdited(_)
            | Message::EditorApply
            | Message::FontSizeEdited(_)
            | Message::FontSizeApply
            | Message::SessionFieldEdited(..)
            | Message::SessionFieldApply(_)) => self.settings_field_message(message),
            message @ (Message::ToolPathEdited(..) | Message::ToolPathApply(_)) => {
                self.tool_path_message(message)
            }
            Message::FileDropped(path) => {
                return self.file_dropped(crate::drop_batch::DropPlace::Main, path);
            }
            Message::DropGathered(crate::drop_batch::DropPlace::Floating(window)) => {
                return self.floating_drop_gathered(window);
            }
            Message::DropGathered(place) if self.hash_drop_target().is_some() => {
                return self.hash_drop_gathered(place);
            }
            message @ (Message::FilesHovered(_) | Message::DropGathered(_)) => {
                self.drop_message(&message)
            }
            Message::OpenWithSystem(target) => return open_with_system(target),
            Message::Tool(tab, message) => {
                return self.tools.update(tab, message, self.main_window);
            }
            Message::ToolsFilter(typed) => {
                self.tools_filter = typed;
                Vec::new()
            }
            Message::ToolsSearch(typed) => {
                self.tools_search = typed;
                Vec::new()
            }
            Message::OpenTool(tool) => {
                // As the C# launch from a card or the sidebar: the Sessions page shown first.
                self.menu = None;
                self.page = Page::Tab;
                self.app.update(AppMessage::Tools(ToolsMessage::Open {
                    tool,
                    title: crate::tools::label(tool),
                }))
            }
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
        self.keep_monitors();
        // The form closed, or back to its protocols: it opens again on General. A gateway
        // edited from it keeps its tab for its return.
        if !self.profile_form_open() {
            self.profile_tab = ProfileTab::General;
            self.profile_refused = false;
        }
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
        // The tool tabs' states follow their tabs.
        self.tools.sync(&self.app);
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
            Message::SettingsTab(tab) => {
                self.settings_tab = tab;
                // A tab chosen is shown whole: the search, and the row outlined, give way.
                self.settings_search.clear();
                self.settings_highlight = None;
            }
            Message::ProfileTab(tab) => {
                self.profile_tab = tab;
                // A tab opens at its top.
                return operation::snap_to(profile_page_id(), RelativeOffset::START);
            }
            Message::ProfilePassword(value) => self.profile_password = Zeroizing::new(value),
            Message::BulkPasswordField { index, value } => {
                if let Some(field) = self.bulk_password.get_mut(index) {
                    *field = Zeroizing::new(value);
                }
                let _ = self
                    .app
                    .update(AppMessage::Selection(SelectionMessage::BulkPasswordEdited));
            }
            Message::FocusBulkPasswordField(index) => {
                return operation::focus(bulk_password_field_id(index));
            }
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
                self.relist_monitors();
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
                main_window_task(self.main_window).and_then(move |id| window::set_mode(id, mode))
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = *modifiers;
                Task::none()
            }
            Message::ManageGateways => {
                self.settings_tab = SettingsTab::Gateways;
                self.view_message(&Message::ShowSettings)
            }
            Message::Navigate(destination) => {
                self.menu = None;
                match destination {
                    Destination::Sessions => self.page = Page::Tab,
                    Destination::Tunnels => self.page = Page::Tunnels,
                    Destination::Tools => self.page = Page::Tools,
                    Destination::About => self.page = Page::About,
                    Destination::Settings => return self.view_message(&Message::ShowSettings),
                }
                Task::none()
            }
            Message::ShowSettings => {
                self.menu = None;
                self.page = Page::Settings {
                    over: self.shown_tab_id(),
                };
                // The trusted keys as they are now: another program may have changed them.
                let _ = self
                    .app
                    .update(AppMessage::Settings(SettingsMessage::TrustedKeys(
                        TrustedKeysMessage::Refresh,
                    )));
                // Whether Windows Hello can unlock the vault, asked again as the C# settings
                // ask it when they load.
                let effects = self
                    .app
                    .update(AppMessage::VaultHello(VaultHelloMessage::Refresh));
                Task::batch(effects.into_iter().map(|effect| self.run(effect)))
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
                    TrustedList::FtpsCertificates => {
                        typed.clone_into(&mut self.ftps_certificate_search);
                    }
                    TrustedList::VncCertificates => {
                        typed.clone_into(&mut self.vnc_certificate_search);
                    }
                }
                Task::none()
            }
            Message::HostKeySort(column) => {
                self.host_key_sort = self.host_key_sort.clicked(*column);
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

    /// Hands the password typed twice in the bulk password dialog to the core; the fields
    /// are emptied either way.
    fn submit_bulk_password(&mut self) -> Vec<Effect> {
        if !matches!(self.app.dialog, Some(Dialog::BulkPassword { .. })) {
            return Vec::new();
        }
        let [password, confirm] = std::mem::take(&mut self.bulk_password);
        let secret = |mut text: Zeroizing<String>| Secret::new(std::mem::take(&mut *text));
        self.app.update(AppMessage::SetBulkPassword {
            password: secret(password),
            confirm: secret(confirm),
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
    fn forms(&self) -> Forms<'_> {
        Forms {
            rdp_resize_delay: self.app.settings().rdp_resize_enable_delay_ms,
            monitors: self.monitors.as_deref().unwrap_or_default(),
            vault: &self.vault_fields,
            profile_password: &self.profile_password,
            sudo_password: &self.sudo_password,
            profile_passphrase: &self.profile_passphrase,
            gateway_password: &self.gateway_password,
            gateway_passphrase: &self.gateway_passphrase,
            gateways: self.app.gateways(),
            tunnel_problem: self.app.tunnel_problem(),
            agent_chip: self.app.agent_chip(),
            bulk_password: &self.bulk_password,
            passwords: self.password_store(),
            profile_tab: self.profile_tab,
            profile_refused: self.profile_refused,
        }
    }

    /// Whether the profile form is open with a protocol chosen, or left for a gateway it
    /// opened.
    fn profile_form_open(&self) -> bool {
        match &self.app.dialog {
            Some(Dialog::EditProfile { draft, .. }) => draft.protocol_chosen,
            Some(Dialog::EditGateway { back, .. }) => back.is_some(),
            _ => false,
        }
    }

    /// Whether a password typed now can be saved.
    fn password_store(&self) -> PasswordStore {
        if self.app.can_save_passwords() {
            PasswordStore::Ready
        } else if self.app.vault_status() == VaultStatus::Locked {
            PasswordStore::VaultLocked
        } else {
            PasswordStore::None
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

    /// Whether Tab and Shift+Tab move the keyboard between the main window's fields: under
    /// a dialog, and on a page of the navigation that is not a tab's (the settings, the
    /// tunnels, About), Quick Connect closed. A tab shown keeps Tab: a terminal's, or a
    /// Files tab's other pane.
    #[must_use]
    pub fn tab_moves_focus(&self) -> bool {
        self.app.dialog.is_some() || (self.palette.is_none() && self.page_over_tab())
    }

    /// Whether a page of the navigation is shown in place of the tab: the settings, the
    /// tunnels or About. The tab's keys are not its then.
    fn page_over_tab(&self) -> bool {
        self.settings_shown() || matches!(self.page, Page::Tunnels | Page::About)
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
        crate::screenshot::copy_session(self.main_window)
            .map(|copied| Message::App(AppMessage::ScreenshotTaken { copied }))
    }

    /// Ctrl+Shift+A: [`Self::status_report`] put on the clipboard, for a screen reader to
    /// read what the C# live region would have announced. Over a dialog too: it reads
    /// nothing the window does not show.
    #[must_use]
    pub fn copy_status(&self) -> Vec<Effect> {
        vec![Effect::WriteClipboard(self.status_report())]
    }

    /// What the status bar says now, then what it said lately, newest first, each with its
    /// time: one plain line each.
    #[must_use]
    pub fn status_report(&self) -> String {
        let targets = self.app.broadcast_target_count();
        let now =
            crate::status_bar::status_text(&self.app.session_status(), self.app.notice(), targets);
        crate::status_bar::status_report(&now, self.app.announcements(), targets)
    }

    fn shortcut(&mut self, shortcut: WindowShortcut) -> Vec<Effect> {
        if shortcut == WindowShortcut::Help {
            self.menu = None;
            return self.app.update(AppMessage::ShowShortcuts);
        }
        if shortcut == WindowShortcut::QuickConnect {
            self.quick_connect_key();
            return Vec::new();
        }
        if shortcut == WindowShortcut::CopyStatus {
            return self.copy_status();
        }
        // The sidebar's other tab, as the C# Ctrl+Shift+T, tab or no tab.
        if shortcut == WindowShortcut::ToggleToolsPanel {
            return self
                .app
                .update(AppMessage::Tools(ToolsMessage::ToggleSidebarTab));
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
        // The strip's tabs: a pane docked in a split is shown with its tab.
        let shown = self.shown_tab_id().unwrap_or(active);
        let strip: Vec<TabId> = self.app.strip().iter().map(|tab| tab.id).collect();
        let count = strip.len();
        let index = strip.iter().position(|id| *id == shown);
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
            // The whole tab, every pane of its split, as the C# Ctrl+W.
            (WindowShortcut::CloseTab, _) => AppMessage::RequestCloseTab(shown),
            (WindowShortcut::NextTab, Some(index)) => {
                AppMessage::SelectTab(strip[(index + 1) % count])
            }
            (WindowShortcut::PreviousTab, Some(index)) => {
                AppMessage::SelectTab(strip[(index + count - 1) % count])
            }
            (WindowShortcut::ToggleSplit, _) => {
                if self.app.shown_tab().is_none_or(|tab| tab.layout.is_none()) {
                    return Vec::new();
                }
                AppMessage::Split(SplitMessage::ToggleAxis(shown))
            }
            // The keyboard to another pane of the split shown; a tab not split keeps it.
            (WindowShortcut::NextPane, _) => AppMessage::Split(SplitMessage::FocusNext(shown)),
            (WindowShortcut::PreviousPane, _) => {
                AppMessage::Split(SplitMessage::FocusPrevious(shown))
            }
            // Settings and the help: shown above, tab or no tab; a screenshot is taken by
            // the window.
            (
                WindowShortcut::Settings
                | WindowShortcut::Help
                | WindowShortcut::Screenshot
                | WindowShortcut::QuickConnect
                | WindowShortcut::CopyStatus
                | WindowShortcut::ToggleToolsPanel,
                _,
            )
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
            self.focus_next = Some(crate::finder::field_id(tab));
        }
    }

    /// A change in the search bar over a tab's terminal: its text, a search, closed. The bar
    /// is the one of the window the tab is drawn in: its own window's when it is detached,
    /// the main window's else; a message naming a tab whose bar is not open is dropped.
    fn finder_message(&mut self, message: Message) -> Vec<Effect> {
        let tab = match &message {
            Message::FinderQuery { tab, .. }
            | Message::FinderFind { tab, .. }
            | Message::FinderClose(tab) => *tab,
            _ => return Vec::new(),
        };
        let slot = if self.app.is_floating(tab) {
            let Some(floating) = self
                .floating_window(tab)
                .and_then(|window| self.floating.get_mut(&window))
            else {
                return Vec::new();
            };
            &mut floating.finder
        } else {
            &mut self.finder
        };
        let Some(finder) = slot.as_mut().filter(|finder| finder.tab == tab) else {
            return Vec::new();
        };
        match message {
            Message::FinderQuery { query, .. } => finder.query = query,
            Message::FinderFind { direction, .. } => {
                finder.searched = Some(finder.query.clone());
                let query = finder.query.clone();
                return self.app.update(AppMessage::FindInTerminal {
                    tab,
                    query,
                    direction,
                });
            }
            _ => *slot = None,
        }
        Vec::new()
    }

    /// Whether `tab`'s session takes the keyboard: its pane has it, no dialog and no Quick
    /// Connect is open, and, in the main window, the tree does not have it.
    fn takes_keys(&self, tab: &Tab, focused: bool) -> bool {
        focused
            && self.app.dialog.is_none()
            && self.palette.is_none()
            && (!self.tree_focused || self.app.is_floating(tab.id))
    }

    /// The search bar over the terminal of `tab`, when open there: its own window's when the
    /// tab is detached, the main window's else.
    fn finder_of(&self, tab: &Tab) -> Option<&Finder> {
        let finder = if self.app.is_floating(tab.id) {
            self.floating_window(tab.id)
                .and_then(|window| self.floating.get(&window))
                .and_then(|floating| floating.finder.as_ref())
        } else {
            self.finder.as_ref()
        };
        finder.filter(|finder| finder.tab == tab.id)
    }

    /// `tab`'s terminal, with its search bar over it when open. Under the bar the terminal
    /// takes no keys: Escape and what is typed are the bar's.
    /// A shell tab's page: its terminal, and its server health panel beside it when shown.
    fn shell_page<'a>(&'a self, tab: &'a Tab, focused: bool) -> Element<'a, Message> {
        let terminal = self.searchable_terminal(tab, self.takes_keys(tab, focused));
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
        let shown: Element<'a, Message> =
            container(self.terminal_view(tab, interactive && finder.is_none()))
                .padding(TERMINAL_MARGIN)
                .into();
        match finder {
            Some(finder) => stack![
                shown,
                crate::finder::view(finder, tab.find_found, self.modifiers.shift())
            ]
            .into(),
            None => shown,
        }
    }

    /// The terminal of `tab`, taking input when `interactive`: at its text size, in the font
    /// every terminal is drawn in, its Ctrl+V as chosen.
    #[must_use]
    pub fn terminal_view<'a>(&self, tab: &'a Tab, interactive: bool) -> TerminalView<'a, Message> {
        TerminalView::new(&tab.terminal, tab.id, Message::App)
            .ctrl_v(self.app.settings().ctrl_v_paste)
            .ctrl_k(self.app.settings().ctrl_k_terminal)
            .interactive(interactive)
            .font(self.terminal_font())
            .font_size(self.font_size(tab.id))
            .on_zoom(|zoom| Message::Shortcut(WindowShortcut::Zoom(zoom)))
    }

    /// The font every terminal is drawn in, those open included: the family the settings
    /// choose when this computer has it, else the embedded one.
    #[must_use]
    pub fn terminal_font(&self) -> TerminalFont {
        TerminalFont::chosen(&self.app.settings().terminal_font_family)
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
        self.open_tree_menu_at(menu, self.cursor.get(), None);
    }

    /// Opens `menu` at `cursor` in `window`, a tab's own window or, `None`, the main one;
    /// for a sub-menu, where its menu was.
    fn open_tree_menu_at(&mut self, menu: TreeMenu, cursor: Point, window: Option<window::Id>) {
        let at = match (&menu, &self.menu) {
            (
                TreeMenu::ConnectAs(_)
                | TreeMenu::ConnectWith(_)
                | TreeMenu::OpenInSplit(_)
                | TreeMenu::MoveFolder(_)
                | TreeMenu::FolderColor(_)
                | TreeMenu::MoveProfile(_)
                | TreeMenu::MoveSelection
                | TreeMenu::EditSelection
                | TreeMenu::GatewaySelection,
                Some((_, at)),
            ) => *at,
            _ => cursor,
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
        self.menu_window = window;
    }

    /// Whether Escape leaves full screen: in it, with no dialog, menu, Quick Connect, search
    /// bar or path bar for Escape to close first.
    fn escape_leaves_fullscreen(&self) -> bool {
        self.fullscreen
            && self.app.dialog.is_none()
            && self.palette.is_none()
            && self.finder.is_none()
            && self.menu.is_none()
            && self.main_path_editing().is_none()
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
            && let Some((tab, side)) = self.main_path_editing()
        {
            self.path_editing = None;
            // Then a path bar typed in, back to the folder shown, as the C# one.
            return self
                .app
                .update(AppMessage::Files(FilesMessage::PathCancelled { tab, side }));
        }
        if self.app.dialog.is_none() {
            // Escape reaches here even when a terminal sent it to its session; in a Files tab
            // it gives up the listing on its way, as the C# one.
            return self.files_key(if confirm {
                FilesKey::Open
            } else {
                FilesKey::CancelLoad
            });
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
            // The password typed twice is here too.
            (true, true) if matches!(self.app.dialog, Some(Dialog::BulkPassword { .. })) => {
                self.submit_bulk_password()
            }
            (true, true) => self.app.update(AppMessage::ConfirmDialog),
            // Only a click agrees to this one.
            (true, false) => Vec::new(),
            (false, _) => self.app.update(AppMessage::DismissDialog),
        }
    }

    /// The Files tab shown and the pane that has the keyboard in it, its lists in sight:
    /// none for another tab, the settings, a dialog or the integrated editor.
    fn shown_files_side(&self) -> Option<(TabId, heimdall_app::files::Side)> {
        if self.settings_shown() || self.app.dialog.is_some() || self.tree_focused {
            return None;
        }
        let tab = self.app.active_tab()?;
        let files = tab.files.as_deref()?;
        files.editor.is_none().then_some((tab.id, files.focus))
    }

    /// The Files pane of the main window whose path bar is typed in: none while it is a
    /// pane of a tab's own window.
    fn main_path_editing(&self) -> Option<(TabId, heimdall_app::files::Side)> {
        self.path_editing
            .filter(|(tab, _)| !self.app.is_floating(*tab))
    }

    /// The path bar of `side` in `tab` given the keyboard, its path shown to type in.
    fn edit_path(&mut self, tab: TabId, side: heimdall_app::files::Side) {
        self.path_editing = Some((tab, side));
        self.focus_next = Some(files_view::field_id(tab, side, files_view::PaneField::Path));
    }

    /// Sends `key` to the tab shown; the core ignores it unless that is a Files tab.
    fn files_key(&mut self, key: FilesKey) -> Vec<Effect> {
        if key == FilesKey::FocusPath {
            if let Some((tab, side)) = self.shown_files_side() {
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
        // A Files tab hidden behind a page: Enter, Delete and the rest are not its.
        if self.page_over_tab() {
            return Vec::new();
        }
        let Some(tab) = self.app.active else {
            return Vec::new();
        };
        self.app
            .update(AppMessage::Files(FilesMessage::Key { tab, key }))
    }

    /// Scrolls the focused list of the Files tab shown so its selection is in view.
    fn reveal_selection(&self) -> Task<Message> {
        self.app
            .active
            .map_or_else(Task::none, |tab| self.reveal_selection_of(tab))
    }

    /// Scrolls the focused list of Files tab `tab` so its selection is in view. The list is
    /// snapped to the selection's share of its length, which keeps a row of equal height
    /// inside the viewport wherever it is.
    fn reveal_selection_of(&self, tab: TabId) -> Task<Message> {
        let Some(files) = self.app.tab(tab).and_then(|found| found.files.as_deref()) else {
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
            files_view::list_id(tab, files.focus),
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
        if !matches!(app.dialog, Some(Dialog::BulkPassword { .. })) {
            self.bulk_password = Default::default();
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

    /// Lists the screens when an RDP profile form opens, and forgets them once it is
    /// closed: the form reads them as they were listed, not at each redraw.
    fn keep_monitors(&mut self) {
        let rdp_form = matches!(
            &self.app.dialog,
            Some(Dialog::EditProfile { draft, .. }) if draft.protocol == DraftProtocol::Rdp
        );
        if !rdp_form {
            self.monitors = None;
        } else if self.monitors.is_none() {
            self.monitors = Some(crate::screens::monitors());
        }
    }

    /// Lists the screens again for an RDP profile form open: the window rescaled or resized,
    /// a screen may have been plugged or removed.
    fn relist_monitors(&mut self) {
        if self.monitors.is_some() {
            self.monitors = Some(crate::screens::monitors());
        }
    }

    /// Gives focus to the first field of the question shown, once per question: the main
    /// window's, and each tab's own window's.
    fn focus_question(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for floating in self.floating.values_mut() {
            let asked = self
                .app
                .floating_tab(floating.key)
                .and_then(|tab| tab.prompts.front())
                .map(|prompt| prompt.question);
            if asked != floating.question {
                floating.question = asked;
                tasks.extend(asked.map(|question| operation::focus(field_id(question, 0))));
            }
        }
        let shown = self
            .app
            .active_tab()
            .and_then(|tab| tab.prompts.front())
            .map(|prompt| prompt.question);
        if shown != self.focused {
            self.focused = shown;
            tasks.extend(shown.map(|question| operation::focus(field_id(question, 0))));
        }
        Task::batch(tasks)
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
            Some(Dialog::BulkPassword { .. }) => {
                (Some(DialogFocus::BulkPassword), bulk_password_field_id(0))
            }
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
        // A form refused shows the tab holding the field to fix.
        if let Some(DialogFocus::FormError(DialogForm::Profile, error)) = next {
            self.profile_tab = ProfileTab::holding(error.field());
            self.profile_refused = true;
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
        // The window the system's dialogs are held by.
        let main = self.main_window;
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
            | Effect::SudoListRemote { .. }
            | Effect::SendEditAnyway { .. }
            | Effect::OpenFolder { .. }
            | Effect::OpenLocalFile { .. }
            | Effect::OpenWithChooser { .. }) => {
                let journal = self.app.operation_journal(&effect);
                files_task(effect, journal)
            }
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
            Effect::CheckForUpdate => Task::future(async {
                // The system's certificates and proxy, then the request: each waits on the
                // system or the network, off the UI thread.
                let outcome = tokio::task::spawn_blocking(|| {
                    heimdall_app::update_check::check(
                        &GitHubSource::new(),
                        std::time::SystemTime::now(),
                    )
                })
                .await
                .unwrap_or_else(|error| {
                    log::warn!("the update check stopped: {error}");
                    Outcome::Failed(UpdateFailure::NetworkUnreachable)
                });
                Message::App(AppMessage::Update(heimdall_app::UpdateMessage::Checked(
                    outcome,
                )))
            }),
            Effect::VerifyWindowsHello => {
                let reason = fl!("ui-windows-hello-verify-reason");
                Task::future(async move {
                    // The prompt holds the call until answered: off the UI thread.
                    let answer = tokio::task::spawn_blocking(move || {
                        windows_hello::ask(&windows_hello::SystemVerifier, &reason)
                    })
                    .await
                    .unwrap_or_else(|error| {
                        log::warn!("the Windows Hello verification stopped: {error}");
                        Err(windows_hello::HelloRefusal::NotVerified)
                    });
                    Message::App(AppMessage::WindowsHello(answer))
                })
            }
            Effect::CheckCredentialGuard(detector) => Task::future(async move {
                // The check runs on a blocking thread of the runtime, bounded in time.
                Message::App(AppMessage::CredentialGuard(detector.status().await))
            }),
            Effect::SyncKnownHosts { source, store } => Task::future(async move {
                // Files read and written: off the UI thread, the window never waiting.
                let _ = tokio::task::spawn_blocking(move || {
                    heimdall_app::known_hosts_sync::run(
                        &source,
                        &heimdall_ssh::KnownHosts::new(store),
                    )
                })
                .await;
            })
            .discard(),
            Effect::LaunchCitrix { tab, name, launch } => {
                crate::citrix_view::launch(tab, name, launch)
            }
            Effect::ProbeCitrix {
                tab,
                launcher,
                lists,
            } => crate::citrix_view::probe(tab, launcher, lists),
            Effect::TerminateCitrix { tab, pid, force } => {
                crate::citrix_view::terminate(tab, pid, force)
            }
            Effect::LaunchRdpExternal {
                name,
                gateway,
                content,
            } => Task::future(async move {
                // Writing the file and starting a process wait on the system: off the UI
                // thread, as a Citrix launch.
                let result = tokio::task::spawn_blocking(move || {
                    heimdall_app::rdp_external::launch(&content)
                })
                .await
                .unwrap_or_else(|error| {
                    Err(heimdall_app::rdp_external::ExternalRefusal::NotStarted(
                        error.to_string(),
                    ))
                });
                Message::App(AppMessage::RdpExternalLaunched {
                    name,
                    gateway,
                    result,
                })
            }),
            Effect::LaunchElevated { tab, request } => Task::future(async move {
                // The elevation prompt holds the call until answered: off the UI thread, on
                // a worker thread of its own, where COM is set up for it.
                let outcome = tokio::task::spawn_blocking(move || {
                    heimdall_app::elevated_shell::launch(&request)
                })
                .await
                .unwrap_or_else(|error| {
                    heimdall_app::elevated_shell::ElevatedOutcome::Failed(error.to_string())
                });
                Message::App(AppMessage::ElevatedLaunched { tab, outcome })
            }),
            Effect::ProbePuttyHostKey { profile, options } => Task::future(async move {
                let profile = *profile;
                let probe = heimdall_app::putty::probe_host_key(profile.clone(), *options).await;
                Message::App(AppMessage::PuttyHostKey {
                    profile: Box::new(profile),
                    probe,
                })
            }),
            Effect::LaunchPutty { name, launch } => Task::future(async move {
                // Finding PuTTY, the X server and starting them wait on the system: off the
                // UI thread, as Remote Desktop Connection.
                let result =
                    tokio::task::spawn_blocking(move || heimdall_app::putty::launch(&launch))
                        .await
                        .unwrap_or_else(|error| {
                            Err(heimdall_app::putty::PuttyRefusal::NotStarted(
                                error.to_string(),
                            ))
                        });
                Message::App(AppMessage::PuttyLaunched { name, result })
            }),
            Effect::OpenPuttyRoute { id, request } => {
                // Its end is PuTTY's: the stream ends once the forward is released.
                let registry = self.registry.clone();
                let events = stream::once(async move {
                    heimdall_app::putty_driver::putty_route_events(
                        *request,
                        registry,
                        heimdall_app::putty::start,
                    )
                })
                .flatten();
                Task::stream(events)
                    .map(move |event| Message::App(AppMessage::PuttyRoute { id, event }))
            }
            Effect::WriteClipboardImage(image) => Task::future(async move {
                let _ = tokio::task::spawn_blocking(move || write_clipboard_image(&image)).await;
            })
            .discard(),
            Effect::WriteFileList(paths) => Task::future(async move {
                let _ = tokio::task::spawn_blocking(move || write_file_list(&paths)).await;
            })
            .discard(),
            Effect::SaveExport { document, count } => save_export(document, count, main),
            Effect::PickOpenSshConfig => pick_openssh(main),
            Effect::PickRdpFiles => pick_rdp(main),
            // Held by the window the Files tab is in, as its C# browser's.
            Effect::PickUploads { tab } => pick_uploads(tab, self.owner_of(tab)),
            Effect::ReadExplorerFiles { tab } => read_explorer_files(tab),
            Effect::ReadDesktopClipboard { tab } => read_desktop_clipboard(tab),
            // Held by the window the desktop is in, as the C# prompts are.
            Effect::PickSaveFolder { tab } => pick_save_folder(tab, self.owner_of(tab)),
            Effect::PickSessionsFile => pick_sessions_file(main),
            Effect::SaveSettingsFile { document } => crate::settings_file::save(document, main),
            Effect::PickSettingsFile => crate::settings_file::pick(main),
            Effect::SettingsReset => {
                self.forget_typed_settings();
                Task::none()
            }
            Effect::PickKnownHosts => pick_known_hosts(main),
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
            // The cache files, read off the window's thread.
            Effect::ScanCitrixCache => Task::perform(
                async {
                    tokio::task::spawn_blocking(heimdall_core::import::citrix_cache::scan)
                        .await
                        .unwrap_or_else(|error| heimdall_core::import::citrix_cache::CacheScan {
                            apps: Vec::new(),
                            warnings: vec![
                                heimdall_core::import::citrix_cache::CacheWarning::Unreadable {
                                    file: String::new(),
                                    detail: error.to_string(),
                                },
                            ],
                        })
                },
                |scan| Message::App(AppMessage::CitrixScanned(scan)),
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
                ticket,
            } => open_vault_task(path, password, job, ticket),
            Effect::CheckVaultHello => Task::perform(
                vault_hello::check(vault_hello::SystemKeyCredentials),
                |available| vault_hello_message(VaultHelloMessage::Checked(available)),
            ),
            Effect::EnrolVaultHello { data_key, previous } => Task::perform(
                vault_hello::enrol_away(vault_hello::SystemKeyCredentials, data_key, previous),
                |result| vault_hello_message(VaultHelloMessage::Enrolled(result)),
            ),
            Effect::UnlockVaultHello { path, envelope } => Task::perform(
                vault_hello::unlock_away(vault_hello::SystemKeyCredentials, path, envelope),
                |result| vault_hello_message(VaultHelloMessage::Unlocked(result)),
            ),
            Effect::DeleteVaultHelloCredential(name) => Task::future(vault_hello::delete_away(
                vault_hello::SystemKeyCredentials,
                name,
            ))
            .discard(),
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
            Effect::OpenWindow(key) => self.open_floating(key),
            Effect::CloseWindow(key) => self.close_floating(key),
            Effect::FocusWindow(key) => self.window_of(key).map_or_else(Task::none, focus_window),
            Effect::FocusMainWindow => {
                main_window_task(main).then(|main| main.map_or_else(Task::none, focus_window))
            }
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
        main_window_task(self.main_window).then(move |id| {
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
    /// The open dialog over the window, veiled; none while a name is typed in the tree's
    /// row, or with no dialog.
    fn dialog_layer(&self, locked: bool) -> Option<Element<'_, Message>> {
        let dialog = self
            .app
            .dialog
            .as_ref()
            .filter(|_| self.inline_rename().is_none())?;
        // Built for the window's height: the profile form scrolls above its buttons.
        Some(opaque(
            container(responsive(move |size| {
                let content = dialog_view(dialog, &self.forms());
                // The OpenSSH preview is a table: wider than a form, as the C# one.
                let card = if matches!(
                    dialog,
                    Dialog::SessionsPreview(_)
                        | Dialog::RdpPreview(_)
                        | Dialog::HostKeysPreview(_)
                        | Dialog::FileConflicts { .. }
                ) {
                    wide_card(content)
                } else if matches!(dialog, Dialog::EditProfile { .. }) {
                    // A margin of a spacing above and below it.
                    form_card(content, size.height - 2.0 * spacing::LG)
                } else {
                    card(content)
                };
                center(card).into()
            }))
            .id(dialog_area_id())
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
        ))
    }

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
            // The banner's place is kept while it is hidden: the page under it keeps its
            // place in the tree, and with it the state of its widgets.
            let banner = self
                .update_banner()
                .unwrap_or_else(|| iced::widget::space().height(0.0).into());
            column![
                self.navigation(),
                banner,
                self.shown_page(),
                self.status_bar()
            ]
            .into()
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
                        button(text(fl!("ui-fullscreen-exit")).size(font_size::CAPTION))
                            .style(styles::secondary)
                            .on_press(Message::ToggleFullscreen),
                        text(fl!("ui-fullscreen-exit-tooltip")).size(font_size::CAPTION),
                        tooltip::Position::Left,
                    )
                    .style(container::rounded_box),
                )
                .padding(spacing::MD)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
            );
        }
        if let Some(layer) = self.dialog_layer(locked) {
            layers = layers.push(layer);
        }
        let open_menu = self
            .menu
            .as_ref()
            .filter(|(menu, _)| !locked && !self.menu_in_floating(menu))
            .and_then(|(menu, at)| Some((self.open_menu_entries(menu)?, *at)));
        if let Some(palette) = self.palette.as_ref().filter(|_| !locked) {
            let results = self
                .app
                .quick_results_in(&palette.query, palette.split.map(|(host, _)| host));
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
        if let Some(overlay) = self.tab_drop_overlay().filter(|_| !locked) {
            layers = layers.push(overlay);
        }
        if let Some(overlay) = self.detach_overlay().filter(|_| !locked) {
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
        container(CursorTracker::new(layers, self.cursor.clone()))
            .id(main_area_id())
            .into()
    }

    /// The banner of a newer release, as the C# one under its navigation: the version, then
    /// "View release", "Later" and "Skip this version". Never in full screen.
    fn update_banner(&self) -> Option<Element<'_, Message>> {
        let release = self.app.update_offer(self.fullscreen)?;
        let send = |message| Message::App(AppMessage::Update(message));
        Some(
            container(
                row![
                    text(fl!("ui-update-banner-text", version = release.to_string())),
                    iced::widget::space::horizontal(),
                    button(text(fl!("ui-update-banner-view-release")))
                        .style(styles::primary)
                        .on_press(send(heimdall_app::UpdateMessage::ViewRelease)),
                    button(text(fl!("ui-update-banner-later")))
                        .style(styles::secondary)
                        .on_press(send(heimdall_app::UpdateMessage::Later)),
                    button(text(fl!("ui-update-banner-skip")))
                        .style(styles::subtle)
                        .on_press(send(heimdall_app::UpdateMessage::Skip)),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            )
            .padding([spacing::MD, spacing::LG])
            .width(Length::Fill)
            .style(styles::strip)
            .into(),
        )
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
        let entries = if let TreeMenu::Tab(tab) | TreeMenu::Pane(tab) = *menu {
            let mut state = self.tab_menu_state(tab)?;
            state.pane = matches!(menu, TreeMenu::Pane(_));
            tree_view::tab_menu_entries(&state)
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
        } else if let TreeMenu::Tool(tool) = *menu {
            tree_view::tool_menu_entries(tool, self.app.is_favorite_tool(tool))
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
                        runs_in_shell: side == Side::Local
                            && self.app.offers_run_in_shell(tab, index),
                    })
                }
                None => None,
            };
            tree_view::files_entry_menu(
                (tab, side),
                facts,
                tree_view::FilesTabFacts {
                    can_paste: self.app.can_paste(tab),
                    can_copy: self.app.can_hold_copy(tab),
                    connected: self.app.files_connected(tab),
                    sftp: self.app.files_over_sftp(tab),
                    over_ssh: self.app.can_copy(tab),
                    local_only: files.local_only,
                },
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
            tree_view::filter_entries(self.app.tree_filter(), self.app.shows_gateway_badge())
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
                self.app.bulk_password_targets(&selected),
                self.password_store().blocked(),
                self.app.gateway_targets(&selected),
            )
        } else if let TreeMenu::GatewaySelection = menu {
            tree_view::gateway_selection_entries(self.app.gateways())
        } else if let TreeMenu::ConnectWith(id) = menu {
            // Only while the session is saved, and an RDP one.
            if self.app.profile_summary(id)?.kind != heimdall_app::ProfileKind::Rdp {
                return None;
            }
            tree_view::connect_with_entries(id)
        } else if let TreeMenu::OpenInSplit(id) = menu {
            // Only while the session is saved and a tab is shown to split.
            self.app.profile_summary(id)?;
            self.app.shown_tab()?;
            tree_view::open_in_split_entries(id)
        } else if let TreeMenu::SplitAxis(tab) = *menu {
            // Only while the tab is there, not split.
            if self.app.tab(tab).is_none() || self.app.in_split(tab) {
                return None;
            }
            tree_view::split_axis_entries(tab)
        } else if let TreeMenu::MergeWith(host) = *menu {
            let tabs: Vec<(TabId, String)> = self
                .app
                .merge_candidates(host)
                .iter()
                .map(|tab| (tab.id, tab.display_title().to_owned()))
                .collect();
            // Only while one can be merged.
            if tabs.is_empty() {
                return None;
            }
            tree_view::merge_with_entries(host, &tabs)
        } else if let TreeMenu::MergeAxis { host, tab } = *menu {
            // Only while it can still be merged.
            if !self
                .app
                .merge_candidates(host)
                .iter()
                .any(|found| found.id == tab)
            {
                return None;
            }
            tree_view::merge_axis_entries(host, tab)
        } else {
            let profile = match menu {
                TreeMenu::Profile(id) | TreeMenu::ConnectAs(id) => self.app.profile_summary(id),
                TreeMenu::Add
                | TreeMenu::More
                | TreeMenu::Filter
                | TreeMenu::Tab(_)
                | TreeMenu::Pane(_)
                | TreeMenu::ConnectWith(_)
                | TreeMenu::OpenInSplit(_)
                | TreeMenu::SplitAxis(_)
                | TreeMenu::MergeWith(_)
                | TreeMenu::MergeAxis { .. }
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
                | TreeMenu::Tool(_)
                | TreeMenu::Tunnel(_) => None,
            };
            let editable = profile.as_ref().is_some_and(|p| self.app.can_edit(&p.id));
            let connect_as = profile
                .as_ref()
                .map(|p| self.app.connect_as_choices(&p.id))
                .unwrap_or_default();
            let splittable = self.app.shown_tab().is_some();
            tree_view::menu_entries(menu, profile.as_ref(), &connect_as, (editable, splittable))
        };
        Some(entries)
    }

    /// No session shown, as the C# window: with no session saved, a welcome and the ways
    /// to add one; otherwise, how to open one.
    fn home(&self) -> Element<'_, Message> {
        if !self.app.profile_summaries().is_empty() {
            // The session selected, as the C# detail panel shows it: at the top left.
            let selected = self
                .app
                .selected_profile
                .as_ref()
                .and_then(|id| self.app.profile_summary(id));
            return match selected {
                Some(profile) => crate::detail_view::view(
                    &profile,
                    self.app.selected_credentials(),
                    self.app.can_edit(&profile.id),
                    self.app.profile_state(&profile.id),
                ),
                None => center(text(fl!("ui-home-select"))).into(),
            };
        }
        center(
            column![
                text(fl!("ui-home-welcome")).size(font_size::TITLE),
                text(fl!("ui-home-subtitle")),
                row![
                    button(text(fl!("ui-home-add-button")))
                        .style(styles::primary)
                        .on_press(Message::App(AppMessage::NewProfile)),
                    button(text(fl!("ui-home-import-button")))
                        .style(styles::secondary)
                        .on_press_maybe(
                            self.app
                                .can_import()
                                .then_some(Message::App(AppMessage::ImportLegacy)),
                        ),
                    // As the C# empty state's "Explore Tools", which opens the Tools page
                    // as the navigation does (`MainWindow.xaml.cs:2331`).
                    button(text(fl!("ui-home-explore-tools-button")))
                        .style(styles::secondary)
                        .on_press(Message::Navigate(Destination::Tools)),
                ]
                .spacing(spacing::SM),
                text(fl!("ui-home-shortcuts")).size(font_size::CAPTION),
            ]
            .spacing(spacing::SM)
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
                    .push(self.app.tunnels_panel().then(|| {
                        crate::tunnels_view::panel(&self.app.tunnels, &self.app.session_routes())
                    }),)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into(),
            Page::Tunnels => {
                crate::tunnels_view::page(&self.app.tunnels, &self.app.session_routes())
            }
            Page::Tools => crate::tools::catalog::page(&self.app, &self.tools_search),
            Page::About => crate::about_view::view(&self.app),
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
            Page::Tools => Destination::Tools,
            Page::About => Destination::About,
            Page::Settings { .. } => Destination::Settings,
        };
        let mut bar = row![
            container(
                text(APP_NAME)
                    .size(font_size::LARGE)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().primary.base.color),
                    })
            )
            .padding(NAV_TITLE_PADDING),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center);
        for destination in Destination::ALL {
            let active = destination == shown;
            // The page shown in the accent colour, a line under it, as the C# tabs.
            let entry = button(text(destination.label()))
                .style(move |theme: &Theme, status| {
                    let mut style = styles::subtle(theme, status);
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
                    .style(styles::secondary)
                    .on_press(Message::TreeShortcut(TreeShortcut::QuickConnect)),
                text(fl!("ui-nav-quick-connect-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        );
        container(bar)
            .padding([spacing::XS, spacing::MD])
            .width(Length::Fill)
            .style(container::bordered_box)
            .into()
    }

    /// A tab still connecting: to what, through which gateways, and a way to stop.
    fn connecting_card<'a>(&self, tab: &'a Tab) -> Element<'a, Message> {
        center(card(
            column![
                text(match (tab.retry, tab.profile.endpoint()) {
                    (Some(retry), _) => reconnecting(retry),
                    (None, Some((host, port))) => fl!(
                        "ui-connect-progress",
                        target = target(host, port, tab.profile.username())
                    ),
                    (None, None) => fl!("ui-local-starting", name = tab.profile.name()),
                }),
                // Through which gateways, as the C# loading overlay says it.
                Some(self.app.tab_route(tab))
                    .filter(|route| !route.is_empty())
                    .map(|route| text(fl!(
                        "ui-connect-via",
                        route = route
                            .iter()
                            .map(|name| server_text(name))
                            .collect::<Vec<_>>()
                            .join(&fl!("ui-route-test-separator"))
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary)),
                button(text(fl!("ui-connect-cancel-button")))
                    .style(styles::secondary)
                    .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
            ]
            .spacing(spacing::SM),
        ))
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
        let tunnels = self.app.live_tunnels();
        crate::status_bar::view(
            crate::status_bar::status_text(&self.app.session_status(), self.app.notice(), targets),
            row![self.tunnels_toggle(), self.broadcast_controls(targets)]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center)
                .into(),
            crate::status_bar::count_text(shown, summaries.len(), !self.search.trim().is_empty()),
            fl!("ui-tunnels-count", count = tunnels),
        )
    }

    /// The tunnels panel's button, as the C# bar's: a lightning bolt and how many tunnels
    /// are open.
    fn tunnels_toggle(&self) -> Element<'_, Message> {
        tooltip(
            container(
                crate::status_bar::glyph_button(
                    Icon::Lightning,
                    Tint::Text,
                    text(self.app.live_tunnels().to_string())
                        .size(font_size::CAPTION)
                        .into(),
                )
                .style(styles::subtle)
                .on_press(Message::App(AppMessage::Tunnel(TunnelMessage::TogglePanel))),
            )
            .id(tunnels_toggle_id()),
            text(fl!("ui-tunnels-toggle-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Top,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Broadcast input's toggle and scope, as the C# bar's: each its glyph before its words,
    /// the toggle red while on.
    fn broadcast_controls(&self, targets: usize) -> Element<'_, Message> {
        let on = self.app.broadcasting();
        let scope = crate::status_bar::scope_label(self.app.settings().broadcast_scope, targets);
        let broadcast = |message| Message::App(AppMessage::Broadcast(message));
        row![
            tooltip(
                crate::status_bar::glyph_button(
                    Icon::Broadcast,
                    if on { Tint::Danger } else { Tint::Text },
                    text(fl!("ui-broadcast-button"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .into(),
                )
                .style(if on {
                    styles::broadcasting
                } else {
                    styles::subtle
                })
                .on_press(broadcast(BroadcastMessage::Toggle)),
                text(if on {
                    fl!("ui-broadcast-on", scope = scope.as_str())
                } else {
                    fl!("ui-broadcast-toggle-tooltip")
                })
                .size(font_size::CAPTION),
                tooltip::Position::Top,
            )
            .style(container::rounded_box),
            tooltip(
                crate::status_bar::glyph_button(
                    Icon::List,
                    Tint::Text,
                    text(scope.clone()).size(font_size::CAPTION).into(),
                )
                .style(styles::subtle)
                .on_press(broadcast(BroadcastMessage::Scope)),
                text(fl!("ui-broadcast-scope-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Top,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        // As the C# sidebar's top: its "Sessions | Tools" tabs, then the button hiding it.
        // A local shell is a Local profile, opened as any session.
        let shown = self.app.sidebar_tab();
        let header = row![
            sidebar_tabs(shown),
            tooltip(
                container(
                    icons::button(Icon::ClosePane)
                        .style(styles::secondary)
                        .on_press(Message::TreeShortcut(TreeShortcut::ToggleSidebar)),
                )
                .id(sidebar_toggle_id()),
                text(fl!("ui-sidebar-hide-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center);
        // As the C# toolbar's lock: there only while a master password is set.
        let actions = (self.app.vault_status() == VaultStatus::Open).then(|| {
            tooltip(
                button(text(fl!("ui-sidebar-lock-button")))
                    .on_press(Message::App(AppMessage::LockVault))
                    .style(styles::secondary),
                text(fl!("ui-sidebar-lock-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
        });
        // The Tools tab: its filter, its context and its tools, as the C#
        // `SidebarToolsContent`, in place of the tree.
        let body = if shown == SidebarTab::Tools {
            column![
                header,
                crate::tools::catalog::sidebar_panel(&self.app, &self.tools_filter)
            ]
            .push(actions)
        } else {
            let list = self.tree_list();
            // A right click beside the rows is the tree's own menu.
            let tree = mouse_area(
                container(styles::scroll(list))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_right_press(Message::OpenTreeMenu(TreeMenu::Add));
            column![header, self.search_box()]
                .push(actions)
                .push(self.filter_feedback())
                .push(tree)
                .push(self.no_folder_zone())
                .push(self.selection_bar())
                .push(self.undo_bar())
        };
        container(body.spacing(spacing::SM).padding(spacing::MD))
            .width(self.sidebar_width)
            .height(Length::Fill)
            .style(container::rounded_box)
            .into()
    }

    /// Under the tree while something is dragged, as the C# one: dropped there, a session
    /// leaves its folder and a folder goes to the top. At the bottom, as the C#, so that
    /// nothing moves under the pointer when it appears.
    fn no_folder_zone(&self) -> Option<Element<'_, Message>> {
        let drag = self.tree_drag.as_ref().filter(|drag| drag.active)?;
        let zone = container(text(fl!("ui-tree-no-folder-zone")).size(font_size::CAPTION))
            .width(Length::Fill)
            .padding([4.0, 6.0])
            .style(container::bordered_box);
        Some(
            tooltip(
                crate::tree_drag::drop_zone(
                    zone.into(),
                    heimdall_app::DropTarget::Folder(heimdall_app::NO_FOLDER.to_owned()),
                    Some(drag),
                ),
                text(fl!("ui-tree-no-folder-zone-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Top,
            )
            .style(container::rounded_box)
            .into(),
        )
    }

    /// Under the tree for 30 seconds after the tree's organization changed, as the C# bar:
    /// what changed, and Undo.
    fn undo_bar(&self) -> Option<Element<'_, Message>> {
        let said = match self.app.undo_offer()? {
            heimdall_app::OrganizationChange::Move => fl!("ui-tree-changed-move"),
            heimdall_app::OrganizationChange::Reorder => fl!("ui-tree-changed-reorder"),
            heimdall_app::OrganizationChange::Rename => fl!("ui-tree-changed-rename"),
            heimdall_app::OrganizationChange::FolderMove => fl!("ui-tree-changed-folder-move"),
            heimdall_app::OrganizationChange::FolderRename => {
                fl!("ui-tree-changed-folder-rename")
            }
        };
        Some(
            row![
                text(said).size(font_size::CAPTION).style(text::secondary),
                button(text(fl!("ui-tree-undo")).size(font_size::CAPTION))
                    .style(styles::subtle)
                    .on_press(Message::App(AppMessage::UndoMove)),
            ]
            .spacing(spacing::XS)
            .align_y(iced::Alignment::Center)
            .wrap()
            .into(),
        )
    }

    /// The row renamed in place, as the C# tree's F2: a session's or a folder's name asked
    /// while its row is shown; elsewhere the name is asked in a dialog.
    fn inline_rename(&self) -> Option<TreeCursor> {
        let row = match self.app.dialog.as_ref()? {
            Dialog::RenameProfile { id, .. } => TreeCursor::Profile(id.clone()),
            Dialog::FolderName {
                naming: FolderNaming::Rename(path),
                ..
            } => TreeCursor::Folder(path.clone()),
            _ => return None,
        };
        (self.page == Page::Tab && !self.sidebar_hidden && self.tree_cursors().contains(&row))
            .then_some(row)
    }

    /// The name of `row` typed in its place, when it is the one renamed: Enter keeps it,
    /// Escape leaves it as it was; a folder's name refused says why under it.
    fn inline_editor(&self, row: &TreeCursor, depth: usize) -> Option<Element<'_, Message>> {
        if self.inline_rename().as_ref() != Some(row) {
            return None;
        }
        let (value, on_input, error): (&str, fn(String) -> Message, _) =
            match self.app.dialog.as_ref()? {
                Dialog::RenameProfile { value, .. } => (
                    value.as_str(),
                    |value| {
                        Message::App(AppMessage::ProfileMenu(ProfileMenuMessage::NameEdited(
                            value,
                        )))
                    },
                    None,
                ),
                Dialog::FolderName { value, error, .. } => (
                    value.as_str(),
                    |value| Message::App(AppMessage::Folder(FolderMessage::NameEdited(value))),
                    error.map(|error| match error {
                        FolderError::Collision => fl!("ui-folder-error-collision"),
                        _ => fl!("ui-folder-error-invalid"),
                    }),
                ),
                _ => return None,
            };
        let editor = column![
            text_input(&fl!("ui-dialog-name-placeholder"), value)
                .style(styles::text_input)
                .id(name_field_id())
                .size(font_size::CAPTION + 1.0)
                .padding([2.0, 4.0])
                .on_input(on_input)
                .on_submit(Message::App(AppMessage::ConfirmDialog)),
        ]
        .push(error.map(|error| text(error).size(font_size::CAPTION).style(text::danger)))
        .spacing(2.0);
        Some(tree_view::indented(editor.into(), depth))
    }

    /// Under the search box, as the C# tree's: a chip per search or filter applied, a click
    /// taking it off; "Reset all filters"; how many sessions pass of how many.
    fn filter_feedback(&self) -> Option<Element<'_, Message>> {
        let filter = self.app.tree_filter();
        let search = self.search.trim();
        let mut chips: Vec<(String, Message)> = Vec::new();
        if !search.is_empty() {
            chips.push((search.to_owned(), Message::Search(String::new())));
        }
        let toggle = |message| Message::App(AppMessage::Filter(message));
        for kind in heimdall_app::ProfileKind::ALL {
            if filter.has_protocol(kind) {
                chips.push((
                    kind.label().to_owned(),
                    toggle(FilterMessage::Protocol(kind)),
                ));
            }
        }
        for (on, label, message) in [
            (
                filter.favorites(),
                fl!("ui-tree-filter-favorites"),
                FilterMessage::Favorites,
            ),
            (
                filter.connected(),
                fl!("ui-tree-filter-connected"),
                FilterMessage::Connected,
            ),
            (
                filter.gateway(),
                fl!("ui-tree-filter-gateway"),
                FilterMessage::Gateway,
            ),
        ] {
            if on {
                chips.push((label, toggle(message)));
            }
        }
        if chips.is_empty() {
            return None;
        }
        let chips = row(chips.into_iter().map(|(label, message)| {
            let remove = fl!("ui-tree-filter-chip-tooltip", filter = label.as_str());
            tooltip(
                button(
                    row![
                        text(label)
                            .size(font_size::CAPTION)
                            .wrapping(text::Wrapping::None),
                        text(fl!("ui-tree-filter-chip-remove")).size(font_size::CAPTION),
                    ]
                    .spacing(spacing::XS),
                )
                .style(styles::secondary)
                .padding([2.0, 6.0])
                .on_press(message),
                text(remove).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into()
        }))
        .spacing(spacing::XS)
        .wrap();
        let shown = self
            .app
            .tree_rows(&self.search)
            .iter()
            .filter(|row| matches!(row, TreeRow::Profile { .. }))
            .count();
        let total = self.app.profile_summaries().len();
        Some(
            column![
                chips,
                button(text(fl!("ui-tree-filter-reset")).size(font_size::CAPTION))
                    .style(styles::subtle)
                    .padding(0.0)
                    .on_press(Message::ResetTreeFilters),
                text(fl!(
                    "ui-tree-filter-result-count",
                    shown = shown,
                    total = total
                ))
                .size(font_size::CAPTION)
                .style(text::secondary),
            ]
            .spacing(spacing::XS)
            .into(),
        )
    }

    /// Under the tree while several sessions are selected, as the C# bulk bar: how many,
    /// then Connect selected, Move and the rest of the selection's menu.
    fn selection_bar(&self) -> Option<Element<'_, Message>> {
        let selected = self.app.selected_profiles();
        if selected.len() < 2 {
            return None;
        }
        let connectable = selected
            .iter()
            .filter(|id| self.app.connects_in_bulk(id))
            .count();
        let action = |label: String, message: Option<Message>| {
            button(text(label).size(font_size::CAPTION))
                .style(styles::subtle)
                .on_press_maybe(message)
        };
        Some(
            column![
                text(fl!("ui-tree-selection-count", count = selected.len()))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                row![
                    action(
                        fl!("ui-selection-connect", count = connectable),
                        (connectable > 0).then_some(Message::App(AppMessage::Selection(
                            heimdall_app::SelectionMessage::Connect
                        ))),
                    ),
                    action(
                        fl!("ui-tree-selection-move"),
                        Some(Message::OpenTreeMenu(TreeMenu::MoveSelection)),
                    ),
                    action(
                        fl!("ui-tree-selection-more"),
                        Some(Message::OpenTreeMenu(TreeMenu::Selection)),
                    ),
                ]
                .wrap(),
            ]
            .spacing(spacing::XS)
            .into(),
        )
    }

    /// The tree's rows, searched and filtered, and what it says when none passes.
    fn tree_list(&self) -> Column<'_, Message> {
        let mut list = Column::new().spacing(tree_view::ROW_GAP);
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
            list = list.push(text(said).size(font_size::CAPTION)).push(
                button(text(way_back).size(font_size::CAPTION))
                    .style(styles::secondary)
                    .on_press(message),
            );
        }
        let badge = self.app.shows_gateway_badge();
        // As the C# tree: folders nested and folded, sub-folders first, "(No Folder)" last.
        list = list.extend(rows.into_iter().map(|row| match row {
            TreeRow::Folder {
                path,
                name,
                depth,
                open,
                count,
            } => {
                if let Some(editor) = self.inline_editor(&TreeCursor::Folder(path.clone()), depth) {
                    return editor;
                }
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
                if let Some(editor) =
                    self.inline_editor(&TreeCursor::Profile(profile.id.clone()), depth)
                {
                    return editor;
                }
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
                let row = tree_view::indented(
                    tree_view::owned_row(&profile, selected, (state, reach), context),
                    depth,
                );
                // Sessions dragged go before or after a session; a folder, into its folder.
                match self.tree_drag.as_ref().filter(|drag| {
                    drag.active && matches!(drag.source, crate::tree_drag::DragSource::Profiles(_))
                }) {
                    Some(drag) => crate::tree_drag::positioned_zone(row, &profile.id, drag),
                    None => crate::tree_drag::drop_zone(row, target, self.tree_drag.as_ref()),
                }
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
    /// ([`SearchKeys`], [`Shell::search_key`]). As the C#'s row: the clear button inside the
    /// box once something is typed, then the filters, Add and More as quiet icon buttons.
    fn search_box(&self) -> Element<'_, Message> {
        let field = tooltip(
            SearchKeys::new(
                text_input(&fl!("ui-tree-search-placeholder"), &self.search)
                    .style(styles::text_input)
                    .id(search_field_id())
                    .size(font_size::BODY)
                    .padding(SEARCH_PADDING)
                    .on_input(Message::Search)
                    .on_submit(Message::SearchSubmit),
                (!self.search.is_empty()).then(|| Message::Search(String::new())),
                Message::SearchDown,
            ),
            text(fl!("ui-tree-search-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let clear = (!self.search.is_empty()).then(|| {
            container(
                tooltip(
                    button(center(icons::icon(
                        Icon::Close,
                        Tint::Text,
                        font_size::SMALL_CAPTION,
                    )))
                    .width(SEARCH_CLEAR_SIDE)
                    .height(SEARCH_CLEAR_SIDE)
                    .padding(0.0)
                    .style(styles::subtle)
                    .on_press(Message::Search(String::new())),
                    text(fl!("ui-tree-search-clear")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            )
            .align_right(Length::Fill)
            .center_y(Length::Fill)
            .padding(SEARCH_CLEAR_PADDING)
        });
        let tool = |glyph: Element<'static, Message>, tip: String, menu: TreeMenu| {
            tooltip(
                button(glyph)
                    .width(icons::BUTTON_SIDE)
                    .height(icons::BUTTON_SIDE)
                    .padding(0.0)
                    .style(styles::subtle)
                    .on_press(Message::OpenTreeMenu(menu)),
                text(tip).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
        };
        let glyph = |icon| center(icons::icon(icon, Tint::Text, icons::GLYPH_SIDE)).into();
        // The filters: a dot in the accent while one leaves sessions out, as the C#'s.
        let filter = if self.app.tree_filter().is_active() {
            center(stack![
                container(icons::icon(Icon::Filter, Tint::Text, icons::GLYPH_SIDE))
                    .padding(FILTER_DOT_SIDE / 2.0),
                container(
                    container(iced::widget::space())
                        .width(FILTER_DOT_SIDE)
                        .height(FILTER_DOT_SIDE)
                        .style(styles::accent_dot),
                )
                .align_right(Length::Fill)
                .align_top(Length::Fill),
            ])
            .into()
        } else {
            glyph(Icon::Filter)
        };
        row![
            stack![field].push(clear).width(Length::Fill),
            row![
                tool(filter, fl!("ui-tree-filter-tooltip"), TreeMenu::Filter),
                container(tool(
                    glyph(Icon::Add),
                    fl!("ui-tree-add-tooltip"),
                    TreeMenu::Add
                ))
                .id(tree_add_id()),
                tool(
                    glyph(Icon::More),
                    fl!("ui-tree-more-tooltip"),
                    TreeMenu::More
                ),
            ]
            .spacing(TOOL_GAP),
        ]
        .spacing(SEARCH_GAP)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The Files tab shown with its session open, the one files dropped on the window go to.
    fn drop_target(&self) -> Option<TabId> {
        self.app
            .active_tab()
            .filter(|tab| !self.settings_shown() && takes_drops(tab))
            .map(|tab| tab.id)
    }

    /// The hash generator's tab shown, which takes a file dropped on the window as the C#
    /// view takes one dropped on it.
    fn hash_drop_target(&self) -> Option<TabId> {
        self.app
            .active_tab()
            .filter(|tab| {
                !self.settings_shown() && tab.tool() == Some(heimdall_app::tools::ToolId::Hash)
            })
            .map(|tab| tab.id)
    }

    /// A drop's files all come on the hash generator: the first hashed, as the C#
    /// `OnDrop` takes `files[0]`.
    fn hash_drop_gathered(&mut self, place: crate::drop_batch::DropPlace) -> Task<Message> {
        let first = self.drops.take(place).into_iter().next();
        match (self.hash_drop_target(), first) {
            (Some(tab), Some(path)) => self.tools.update(
                tab,
                crate::tools::ToolMessage::Hash(crate::tools::HashMessage::Picked(Some(path))),
                self.main_window,
            ),
            _ => Task::none(),
        }
    }

    /// Files dragged over the window, or a drop's files all come: sent together to the
    /// server's folder of the Files tab shown, as the C# tab takes what Explorer drops on it.
    fn drop_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::FilesHovered(over) => {
                self.files_hovered = over;
                Vec::new()
            }
            Message::DropGathered(place) => {
                let paths = self.drops.take(place);
                if paths.is_empty() {
                    return Vec::new();
                }
                match self.drop_target() {
                    Some(tab) => self
                        .app
                        .update(AppMessage::Files(FilesMessage::Dropped { tab, paths })),
                    // Where no Files tab takes them, .rdp files are imported, as the C# window
                    // takes them dropped anywhere.
                    None => self
                        .app
                        .update(AppMessage::Rdp(heimdall_app::RdpMessage::Dropped(paths))),
                }
            }
            _ => Vec::new(),
        }
    }

    /// "Drop files to upload" over the Files tab shown while files are dragged over it.
    fn drop_overlay(&self) -> Option<Element<'_, Message>> {
        self.drop_target().filter(|_| self.files_hovered)?;
        Some(drop_layer())
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

    /// Where the keyboard goes after `message`: to the tree after a click in it, back to the
    /// session after a tab is chosen; and the page a chosen tab shows.
    fn note_focus(&mut self, message: &Message) {
        match message {
            // A tab detached is shown in its own window: the main window stays as it is.
            Message::App(AppMessage::SelectTab(tab)) if !self.app.is_floating(*tab) => {
                self.page = Page::Tab;
                self.tree_focused = false;
            }
            // A tab's menu is the tab bar's, an entry's the Files tab's: the keyboard stays
            // where it was.
            Message::OpenTreeMenu(
                TreeMenu::Tab(_)
                | TreeMenu::Pane(_)
                | TreeMenu::FilesEntry { .. }
                | TreeMenu::FilesBookmarks(_)
                | TreeMenu::FilesBookmarksRemove(_),
            ) => {}
            Message::App(AppMessage::ToggleFolder(_))
            | Message::OpenTreeMenu(_)
            | Message::TreeClick(_)
            // The desktop takes no more keys: the tree has them, as after a click in it.
            | Message::ContentRelease => self.tree_focused = true,
            // A press in a pane of the split shown gives the keyboard back from the tree too.
            Message::ContentFocus | Message::App(AppMessage::Split(SplitMessage::Focus(_))) => {
                self.tree_focused = false;
            }
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
                    .quick_results_in(&palette.query, palette.split.map(|(host, _)| host))
                    .into_iter()
                    .nth(index)
                {
                    // In split mode, merged into the tab its "Split..." was chosen from.
                    self.app.update(match palette.split {
                        Some((host, axis)) => {
                            AppMessage::Split(SplitMessage::QuickConnect { host, axis, result })
                        }
                        None => AppMessage::QuickConnect(result),
                    })
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

    /// Ctrl+K or Ctrl+Shift+K, as the C# palette's: Quick Connect opened over whatever has
    /// the keyboard, a session included; not over a dialog. Open already, it keeps what was
    /// typed and takes the keyboard back.
    fn quick_connect_key(&mut self) {
        if self.palette.is_some() {
            self.focus_next = Some(crate::palette::field_id());
        } else {
            let _ = self.tree_shortcut(TreeShortcut::QuickConnect);
        }
    }

    /// Opens Quick Connect, closing the menu: for a session of its own, or, with `split`,
    /// merged into a tab, as the C# palette's split mode.
    fn open_palette(&mut self, split: Option<(TabId, Axis)>) {
        self.menu = None;
        self.palette = Some(Palette {
            split,
            ..Palette::default()
        });
        self.focus_next = Some(crate::palette::field_id());
    }

    /// A key while Quick Connect is open: the arrows move its choice within its results,
    /// Enter opens the one chosen, the others are nobody's; `None` while it is closed.
    fn palette_key(&mut self, key: FilesKey) -> Option<Vec<Effect>> {
        let palette = self.palette.as_mut()?;
        let count = self
            .app
            .quick_results_in(&palette.query, palette.split.map(|(host, _)| host))
            .len();
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
    /// A tab pressed, dragged along the bar and let go over another, as the C# tab.
    fn tab_drag_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::TabHover(tab) => self.tab_hover = Some(tab),
            Message::TabHoverLeft(tab) => {
                if self.tab_hover == Some(tab) {
                    self.tab_hover = None;
                }
            }
            Message::TabDropArea(area) => {
                // Only for the drag that asked.
                if self.tab_drag.is_some_and(|drag| drag.active) {
                    self.tab_drop_area = area;
                }
            }
            Message::TabDragEnd => {
                let area = self.tab_drop_area.take();
                let outside = self.tab_drag_outside();
                let Some(drag) = self.tab_drag.take() else {
                    return Vec::new();
                };
                // Out of the window: to a window of its own, as the C# drop no target
                // takes; a split tab refused, and said.
                if outside.is_some() {
                    return self.detach_dropped(drag.tab);
                }
                if let Some(onto) = drag.onto(self.tab_hover) {
                    return self.app.update(AppMessage::MoveTab {
                        tab: drag.tab,
                        onto,
                    });
                }
                // Over the content: the tab shown split, or, full already, said.
                if let Some((host, (axis, placement))) = self.tab_drop(drag, area) {
                    return self.app.update(AppMessage::Split(SplitMessage::Merge {
                        host,
                        tab: drag.tab,
                        axis,
                        placement,
                    }));
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Detaches `tab`, let go out of the window, as the menu's "Detach to Window" does; its
    /// window opens where the pointer is, when the screens are known.
    fn detach_dropped(&mut self, tab: TabId) -> Vec<Effect> {
        let effects = self
            .app
            .update(AppMessage::Float(FloatMessage::Detach(tab)));
        let opened = effects.iter().find_map(|effect| match effect {
            Effect::OpenWindow(key) => Some(*key),
            _ => None,
        });
        self.detach_place = opened.zip(crate::screens::pointer());
        effects
    }

    /// The tab dragged, when the pointer is out of the main window by more than
    /// [`crate::tab_drag::DETACH_MARGIN`]: let go there, it goes to a window of its own.
    fn tab_drag_outside(&self) -> Option<crate::tab_drag::TabDrag> {
        let drag = self.tab_drag.filter(|drag| drag.active)?;
        let window = self.window_extent?;
        crate::tab_drag::beyond_window(window, drag.at()).then_some(drag)
    }

    /// Escape, a tab dragged: the drag given up, as the C# `QueryContinueDrag`; let go, the
    /// tab stays where it was. Whether there was one to give up.
    fn cancel_tab_drag(&mut self) -> bool {
        if !self.tab_drag.is_some_and(|drag| drag.active) {
            return false;
        }
        self.tab_drag = None;
        self.tab_drop_area = None;
        true
    }

    /// The pointer moved, a press on a tab held: once it is a drag, where the content is
    /// drawn is asked, for a drop there to split the tab shown, and how large the window
    /// is, for a drop out of it to detach the tab. The content is not watched for the
    /// pointer: a terminal or a desktop takes its moves.
    fn tab_drag_moved(&mut self, at: Point) -> Task<Message> {
        let Some(drag) = self.tab_drag.as_mut() else {
            return Task::none();
        };
        let started = !drag.active;
        drag.moved(at);
        if started && drag.active {
            return Task::batch([
                crate::screenshot::area_bounds().map(Message::TabDropArea),
                main_window_task(self.main_window)
                    .and_then(window::size)
                    .map(Message::WindowResized),
            ]);
        }
        Task::none()
    }

    /// The tab shown, and how `drag` let go over the content drawn in `area` would split it:
    /// none over the tab bar, off the content, onto itself, for a tab split already, or
    /// over another page than the sessions'.
    fn tab_drop(
        &self,
        drag: crate::tab_drag::TabDrag,
        area: Option<Rectangle>,
    ) -> Option<(TabId, (Axis, Placement))> {
        if !drag.active || self.tab_hover.is_some() || self.page != Page::Tab {
            return None;
        }
        let shown = self.app.shown_tab()?;
        let dragged = self.app.tab(drag.tab)?;
        if shown.id == drag.tab || dragged.layout.is_some() {
            return None;
        }
        Some((shown.id, crate::tab_drag::drop_zone(area?, drag.at())?))
    }

    /// "Drop to split" over the half of the content a tab dragged there would take, as the
    /// C# `ContentDropZone`; none where a drop would not split, nor over a tab split in
    /// [`MAX_PANES`] already, where a drop says so.
    fn tab_drop_overlay(&self) -> Option<Element<'_, Message>> {
        let drag = self.tab_drag?;
        let area = self.tab_drop_area?;
        let (host, zone) = self.tab_drop(drag, Some(area))?;
        if self.app.panes_of(host).len() >= MAX_PANES {
            return None;
        }
        let half = crate::tab_drag::drop_half(area, zone);
        Some(drop_zone_overlay(fl!("ui-split-drop-to-split"), half))
    }

    /// "Release to detach to a window" over the whole content while a tab dragged is out of
    /// the window beyond [`crate::tab_drag::DETACH_MARGIN`], the "Drop to split" overlay
    /// gone; none for a tab that cannot go, a split one, whose drop says why. The C# shows
    /// only the system's drag pointer there.
    fn detach_overlay(&self) -> Option<Element<'_, Message>> {
        let drag = self.tab_drag_outside()?;
        let area = self.tab_drop_area?;
        let tab = self.app.tab(drag.tab)?;
        if !self.app.can_detach(tab) {
            return None;
        }
        Some(drop_zone_overlay(fl!("ui-tab-drag-detach-hint"), area))
    }

    fn files_drag_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::FilesHover(spot) => {
                self.files_hover = Some(spot);
                // The main window's, unless a tab's own window says it is its own.
                self.files_hover_window = None;
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
                self.tab_drop_area = None;
                self.tab_drag = self
                    .tab_hover
                    .map(|tab| crate::tab_drag::TabDrag::pressed(tab, self.cursor.get()));
                self.files_drag = self
                    .files_hover
                    .filter(|spot| spot.index.is_some() && !self.app.is_floating(spot.tab))
                    .map(|spot| crate::files_drag::FilesDrag::pressed(spot, self.cursor.get()));
                self.files_drag_window = None;
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
                self.relist_monitors();
                if !self.fullscreen {
                    self.window_size = Some(size);
                }
                self.window_extent = Some(size);
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
    /// type-ahead; the same letter again goes on to the next one. Else the focused list of
    /// the Files tab shown searches it.
    fn type_ahead(&mut self, typed: &str) -> Vec<Effect> {
        if self.app.dialog.is_some() || self.gated() {
            return Vec::new();
        }
        if !self.tree_focused {
            return self.files_type_ahead(typed);
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
                self.open_palette(None);
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
            FilesKey::Parent | FilesKey::Lower => self.nudge(cursor, key == FilesKey::Lower),
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

    /// The session the keyboard is on, one place up or `down`, as the C# Alt+Up and
    /// Alt+Down; a folder does not move.
    fn nudge(&mut self, cursor: Option<TreeCursor>, down: bool) -> Vec<Effect> {
        match cursor {
            Some(TreeCursor::Profile(id)) => self.app.update(AppMessage::NudgeProfile { id, down }),
            _ => Vec::new(),
        }
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
        let route = self.app.tab_route(tab);
        Some(report::error_report(
            self.app.tab_kind(tab).label(),
            server.as_deref(),
            report::Session {
                route: &route,
                lasted: tab.session_lasted(),
            },
            &texts::error(error),
            now,
        ))
    }

    /// The anonymized report of tab `id`'s failure at `now`: when, how many gateways, how
    /// long it was connected, which version, the kind of failure; `None` unless its session
    /// failed.
    #[must_use]
    pub fn anonymous_report(&self, id: TabId, now: std::time::SystemTime) -> Option<String> {
        let tab = self.app.tab(id)?;
        let Phase::Failed(error) = &tab.phase else {
            return None;
        };
        // The variant's name alone: its fields can name the server or the account.
        let shown = format!("{error:?}");
        let kind = shown
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or_default();
        let route = self.app.tab_route(tab);
        Some(report::anonymous_report(
            self.app.tab_kind(tab).label(),
            report::Session {
                route: &route,
                lasted: tab.session_lasted(),
            },
            kind,
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
            pane: false,
            docked: self.app.is_docked(id),
            detach: self.app.can_detach(tab),
            detach_secondary: self
                .app
                .host_of(id)
                .and_then(|host| self.app.detachable_secondary(host))
                .is_some(),
            split: match self.app.host_of(id).filter(|_| self.app.in_split(id)) {
                Some(host) => SplitEntries::Split(host),
                None => SplitEntries::Merge(!self.app.merge_candidates(id).is_empty()),
            },
            transcript: if tab.transcript.is_some() {
                TranscriptEntry::Stop
            } else if shows_terminal(tab) {
                TranscriptEntry::Start(self.app.can_start_transcript(tab))
            } else {
                TranscriptEntry::Absent
            },
        })
    }

    /// A tab of the bar: a click shows it, a right click opens its menu; pressed and moved,
    /// it is dragged, the tab it would take the place of outlined.
    fn tab_button<'a>(
        &self,
        tab: TabId,
        label: iced::widget::Row<'a, Message>,
        active: bool,
    ) -> Element<'a, Message> {
        let dragging = self.tab_drag.filter(|drag| drag.active);
        // As the C# tab: its heading, and under it the accent's line once selected.
        let content = column![
            container(label.align_y(iced::Alignment::Center)).padding(TAB_HEADING_PADDING),
            container(iced::widget::space())
                .width(Length::Fill)
                .height(TAB_UNDERLINE)
                .style(styles::underline(active)),
        ]
        .width(Length::Shrink);
        let mut area = mouse_area(
            button(content)
                .padding(TAB_PADDING)
                .style(styles::tab(active))
                .on_press(Message::App(AppMessage::SelectTab(tab))),
        )
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Tab(tab)))
        .on_enter(Message::TabHover(tab))
        .on_exit(Message::TabHoverLeft(tab));
        if dragging.is_some() {
            area = area.interaction(iced::mouse::Interaction::Grabbing);
        }
        // Where a dragged tab goes, outlined as the C# drop target.
        let target = dragging.and_then(|drag| drag.onto(self.tab_hover)) == Some(tab);
        container(area)
            .style(move |theme: &Theme| container::Style {
                border: iced::Border {
                    color: if target {
                        theme.extended_palette().primary.strong.color
                    } else {
                        Color::TRANSPARENT
                    },
                    width: TAB_DROP_EDGE,
                    radius: radius::SM.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The gateways `tab` goes through, as the C# says them: "via" and their names; `None`
    /// when it goes straight.
    fn route_text(&self, tab: &Tab) -> Option<String> {
        let route = self.app.tab_route(tab);
        if route.is_empty() {
            return None;
        }
        let names = route
            .iter()
            .map(|name| server_text(name))
            .collect::<Vec<_>>()
            .join(&fl!("ui-route-test-separator"));
        Some(fl!("ui-connect-via", route = names))
    }

    /// "via" when `tab` goes through gateways, naming them when pointed at; nothing when it
    /// goes straight.
    fn route_badge(&self, tab: &Tab) -> Option<Element<'static, Message>> {
        let route = self.route_text(tab)?;
        Some(
            tooltip(
                text(fl!("ui-tab-route-badge")).size(font_size::CAPTION),
                text(route).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into(),
        )
    }

    /// The title of `tab`, as its tab and its pane's header name it: a Files tab says so,
    /// and a desktop "Connect with" opened embedded this once, as the C# title suffix, which
    /// a name the user gave replaces (`SessionTabViewModel.cs:255-269`). Remote Desktop
    /// Connection opens no tab to name.
    fn tab_name(tab: &Tab) -> String {
        let name = if tab.files.is_some() && tab.custom_title.is_none() {
            fl!("ui-tab-files-title", name = tab_label(tab.display_title()))
        } else {
            tab_label(tab.display_title())
        };
        if tab.custom_title.is_none()
            && tab.rdp_mode_override == Some(heimdall_core::profile::RdpMode::Embedded)
        {
            fl!("ui-tab-rdp-forced-embedded-title", name = name)
        } else {
            name
        }
    }

    /// What a tab says of itself, as the C# tab: its protocol's icon, its pin, its title
    /// and a dot while it connects, then its marks.
    fn tab_title(&self, tab: &Tab, active: bool) -> iced::widget::Row<'static, Message> {
        let kind = self.app.tab_kind(tab);
        // A tool's tab bears the tool's icon, in its category's colour.
        let (glyph, tint) = match tab.tool() {
            Some(tool) => (crate::tools::icon(tool), Tint::Tool(tool.category())),
            None => (Icon::of(kind), Tint::Protocol(kind)),
        };
        let mut label = row![icons::icon(glyph, tint, icons::GLYPH_SIDE)]
            .spacing(spacing::XS)
            .align_y(iced::Alignment::Center);
        if tab.pinned {
            label = label.push(
                tooltip(
                    icons::icon(Icon::Pin, Tint::Accent, TAB_PIN_SIDE),
                    text(fl!("ui-tab-pinned-badge")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        label = label.push(
            text(Self::tab_name(tab))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(text::base),
        );
        // While it connects, as the C# tab's busy dot.
        let state = SessionState::of(tab);
        if matches!(state, SessionState::Connecting | SessionState::Reconnecting) {
            label = label.push(tree_view::state_dot(Some(state)));
        }
        // Through gateways, said on the tab while the tunnels panel that lists it is
        // closed, as the C# tab's tunnel badge.
        if !self.app.tunnels_panel() {
            label = label.push(self.route_badge(tab));
        }
        for badge in tab_badges(tab, active) {
            label = label.push(badge);
        }
        label
    }

    /// What goes before a tab, on the strip or in a pane's header: its broadcast marker
    /// while broadcasting to the tabs chosen, as the C# tab's, and its post-connect steps.
    fn tab_marks<'a>(&'a self, tab: &'a Tab) -> Vec<Element<'a, Message>> {
        let mut marks = Vec::new();
        let marked = self.app.broadcasting()
            && self.app.settings().broadcast_scope == BroadcastScope::SelectedTabs
            && shows_terminal(tab);
        if marked {
            // A target of broadcast input, or not, as the C# tab's marker.
            let target = self.app.is_broadcast_target(tab.id);
            marks.push(
                tooltip(
                    button(
                        text(if target {
                            fl!("ui-broadcast-target-on")
                        } else {
                            fl!("ui-broadcast-target-off")
                        })
                        .size(font_size::CAPTION),
                    )
                    .style(styles::subtle)
                    .on_press(Message::App(AppMessage::Broadcast(
                        BroadcastMessage::Target(tab.id),
                    ))),
                    text(fl!("ui-broadcast-target-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box)
                .into(),
            );
        }
        if let Some(progress) = &tab.post_connect {
            marks.push(post_connect_badge(tab.id, progress));
        }
        marks
    }

    /// What a tab says of itself on the strip: its title and marks, the transcript it keeps
    /// among them.
    fn tab_heading(&self, tab: &Tab, active: bool) -> iced::widget::Row<'static, Message> {
        self.tab_title(tab, active).push(transcript_mark(tab))
    }

    fn tab_bar(&self) -> Element<'_, Message> {
        let mut tabs = row![]
            .spacing(TAB_GAP)
            .padding(TAB_STRIP_PADDING)
            .align_y(iced::Alignment::Center);
        // The sidebar hidden, a way to show it again, as the C# button where it was.
        if self.sidebar_hidden {
            tabs = tabs.push(
                tooltip(
                    container(
                        icons::button(Icon::ChevronRight)
                            .style(styles::secondary)
                            .on_press(Message::TreeShortcut(TreeShortcut::ToggleSidebar)),
                    )
                    .id(sidebar_toggle_id()),
                    text(fl!("ui-sidebar-show-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        let shown = self.shown_tab_id();
        for tab in self.app.strip() {
            let active = shown == Some(tab.id);
            for mark in self.tab_marks(tab) {
                tabs = tabs.push(mark);
            }
            // The close button inside the tab, at its right, as the C# tab's: a circled cross,
            // faded but on the tab selected or pointed at. It takes the press, which selecting
            // the tab then does not see.
            let quiet = !active && self.tab_hover != Some(tab.id);
            let close = container(
                button(center(icons::faded(
                    Icon::Close,
                    Tint::Text,
                    font_size::SMALL_CAPTION,
                    if quiet {
                        styles::CLOSE_REST_OPACITY
                    } else {
                        1.0
                    },
                )))
                .width(TAB_CLOSE_SIDE)
                .height(TAB_CLOSE_SIDE)
                .padding(0.0)
                .style(styles::close_mark(quiet))
                .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
            )
            .id(tab_close_id(tab.id));
            let label = self.tab_heading(tab, active).push(close);
            tabs = tabs.push(self.tab_button(tab.id, label, active));
        }
        tabs.wrap().into()
    }

    /// A Files tab's page: its integrated editor when a file is open in it, else its lists,
    /// the server's beside the notice its transport discloses: plain FTP sends everything
    /// in clear, as the C# badge says (`EmbeddedSftpView.xaml:161-186`).
    fn files_page<'a>(
        &'a self,
        tab: &Tab,
        pane: &'a heimdall_app::files::FilesPane,
        live: bool,
    ) -> Element<'a, Message> {
        let notice = tab
            .sent_in_clear()
            .then(|| fl!("ui-files-ftp-cleartext-badge"));
        let tab = tab.id;
        match &pane.editor {
            Some(edit) => crate::integrated_editor::view(
                tab,
                edit,
                self.editors.get(edit.id),
                pane.client.is_some(),
                self.editor_syntax(),
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
                self.file_columns.get(&tab).copied().unwrap_or_default(),
                notice,
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
        let Some(tab) = self.app.shown_tab() else {
            return self.home();
        };
        if let Some(layout) = &tab.layout
            && let Some(split) = self.split_page(tab.id, layout)
        {
            return split;
        }
        self.tab_page(tab, true)
    }

    /// The panes of split tab `host`, each with its header; `None` when one of them is gone.
    fn split_page<'a>(
        &'a self,
        host: TabId,
        layout: &'a SplitLayout,
    ) -> Option<Element<'a, Message>> {
        let leaves = layout.leaves();
        let tabs = leaves
            .iter()
            .map(|id| self.app.tab(*id))
            .collect::<Option<Vec<&Tab>>>()?;
        let active = self.app.active;
        let panes = tabs
            .into_iter()
            .map(|tab| {
                let focused = active == Some(tab.id);
                let mut badges = tab_badges(tab, focused);
                badges.extend(transcript_mark(tab));
                split_view::pane(
                    split_view::Heading {
                        tab: tab.id,
                        host,
                        marks: self.tab_marks(tab),
                        name: Self::tab_name(tab),
                        detail: tab_endpoint(tab),
                        route: self.route_text(tab),
                        state: crate::floating_view::state_text(SessionState::of(tab)),
                        badges,
                    },
                    self.tab_page(tab, focused),
                    focused,
                )
            })
            .collect();
        let live = self
            .split_drag
            .filter(|(dragged, ..)| *dragged == host)
            .map(|(_, divider, ratio)| (divider, ratio));
        let focused = leaves.iter().position(|id| active == Some(*id));
        Some(
            SplitView::new(Shape::of(&layout.root, live), panes)
                .id(split_view::area_id())
                .focused(focused)
                .on_focus(move |pane| {
                    Message::App(AppMessage::Split(SplitMessage::Focus(leaves[pane])))
                })
                .on_drag(move |divider, ratio| Message::SplitDragged {
                    host,
                    divider,
                    ratio,
                })
                .on_release(move |divider, ratio| Message::SplitReleased {
                    host,
                    divider,
                    ratio,
                })
                .on_reset(move |_| Message::App(AppMessage::Split(SplitMessage::ResetRatio(host))))
                .into(),
        )
    }

    /// A divider dragged: drawn where it is held, then kept once let go. The application
    /// keeps the share of a tab's outer split, the one divider of two panes.
    fn split_drag_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::SplitDragged {
                host,
                divider,
                ratio,
            } => {
                self.split_drag = Some((host, divider, ratio));
                Vec::new()
            }
            Message::SplitReleased {
                host,
                divider,
                ratio,
            } => {
                self.split_drag = None;
                if divider == split_view::OUTER_DIVIDER {
                    self.app
                        .update(AppMessage::Split(SplitMessage::Resize { host, ratio }))
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// What `tab` shows: its question, its session, or what became of it; a tool's tab, the
    /// tool, as the C# tool view in its tab. Only the pane with the keyboard, `focused`,
    /// takes typing.
    #[expect(clippy::too_many_lines, reason = "one arm per state of a tab")]
    fn tab_page<'a>(&'a self, tab: &'a Tab, focused: bool) -> Element<'a, Message> {
        if let TabProfile::Tool(tool) = tab.profile {
            let theme = self.theme();
            return crate::tools::view(tab.id, tool, self.tools.get(tab.id), &theme);
        }
        // A Citrix application's window is Citrix's own: its tab shows its status alone.
        if let (TabProfile::Citrix(profile), Some(pane)) = (&tab.profile, tab.citrix.as_deref()) {
            let offer = self
                .app
                .citrix_terminate_offer(tab.id, std::time::Instant::now());
            return crate::citrix_view::view(profile, pane, SessionState::of(tab), tab.id, &offer);
        }
        if let Some(prompt) = tab.prompts.front() {
            return center(card(self.question(tab, prompt))).into();
        }
        if let Some(pane) = &tab.elevated {
            return self.elevated_card(tab, pane);
        }
        match &tab.phase {
            Phase::Connecting => self.connecting_card(tab),
            Phase::HostKey {
                host,
                port,
                fingerprint,
            } => host_key_card(
                tab.id,
                (host, *port),
                (fingerprint, tab.host_key_algorithm()),
                tab.asks_about_certificate()
                    .then(|| (tab.profile.name(), tab.certificate_context.as_ref())),
                self.certificate_owner(tab),
            ),
            Phase::Connected => match (tab.files.as_deref(), tab.desktop.as_deref()) {
                (Some(pane), _) => self.files_page(tab, pane, tab.is_live()),
                (_, Some(pane)) => self.desktop(tab, pane, focused),
                _ => self.shell_page(tab, focused),
            },
            // An editor's text outlives its session: kept in sight, saved once connected
            // again.
            Phase::Closed { .. }
                if let Some(edit) = tab.files.as_deref().and_then(|pane| pane.editor.as_ref()) =>
            {
                column![
                    crate::integrated_editor::view(
                        tab.id,
                        edit,
                        self.editors.get(edit.id),
                        false,
                        self.editor_syntax(),
                    ),
                    self.session_actions(tab),
                ]
                .spacing(spacing::SM)
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
                let mut ended = column![text(said)].spacing(spacing::SM);
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
                    self.searchable_terminal(tab, self.app.dialog.is_none() && focused),
                    row![text(status), self.session_actions(tab)]
                        .spacing(spacing::SM)
                        .padding(spacing::MD)
                        .align_y(iced::Alignment::Center),
                ];
                // What PowerShell's error meant, as the C# says it under the session.
                if let Some(found) = tab.winrm_diagnostic {
                    ended = ended.push(
                        container(text(texts::winrm_diagnostic(found)).style(text::danger))
                            .padding(spacing::MD),
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
                    column![text(said), self.session_actions(tab).wrap()].spacing(spacing::SM),
                ))
                .into()
            }
            // Dropped once open: what it showed stays in sight, the failure under it.
            Phase::Failed(error)
                if tab.dropped() && matches!(tab.purpose, Purpose::Shell | Purpose::Files) =>
            {
                self.dropped_page(tab, error, focused)
            }
            Phase::Failed(_) if let Some(retry) = tab.retry => countdown_card(tab.id, retry),
            Phase::Failed(error) => self.failure_card(tab, error),
        }
    }

    /// A local shell run as administrator: no terminal, as Windows runs it in a window of its
    /// own, but what became of its start under an ADMIN badge, as the C# says it; then the way
    /// to open it again, once it can be, and Close.
    fn elevated_card<'a>(&'a self, tab: &'a Tab, pane: &'a ElevatedPane) -> Element<'a, Message> {
        let name = server_text(tab.profile.name());
        let (said, failed) = match &pane.state {
            ElevatedState::Starting => (
                fl!("ui-local-elevated-starting", name = name.as_str()),
                false,
            ),
            ElevatedState::Started => (
                fl!("ui-local-elevated-started", name = name.as_str()),
                false,
            ),
            ElevatedState::Cancelled => (fl!("ui-local-elevated-cancelled"), false),
            ElevatedState::Failed(detail) => (
                fl!("ui-local-elevated-failed", detail = detail.as_str()),
                true,
            ),
            ElevatedState::Unsupported => (fl!("ui-local-elevated-unsupported"), true),
        };
        let badge = container(
            text(fl!("ui-local-admin-badge"))
                .size(font_size::BADGE)
                .style(text::danger)
                .wrapping(text::Wrapping::None),
        )
        .padding([0.0, spacing::XS])
        .style(container::bordered_box);
        let mut body = column![
            row![badge, text(name).size(font_size::TITLE)]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            if failed {
                text(said).style(text::danger)
            } else {
                text(said)
            },
        ]
        .spacing(spacing::SM);
        if let Some(program) = &pane.program {
            body = body.push(
                text(fl!("ui-local-elevated-program", program = program.as_str()))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        let mut actions = row![].spacing(spacing::SM).align_y(iced::Alignment::Center);
        if pane.state != ElevatedState::Unsupported && self.app.can_reconnect(tab) {
            actions = actions.push(
                button(action_label(fl!("ui-local-elevated-open-again-button")))
                    .style(styles::primary)
                    .on_press(Message::App(AppMessage::ReconnectTab(tab.id))),
            );
        }
        actions = actions.push(
            button(action_label(fl!("ui-session-close-button")))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
        );
        center(card(body.push(actions.wrap()))).into()
    }

    /// A session that dropped once open, as the C# "View output" leaves its output readable
    /// after a drop: its terminal, scrolled, selected and copied as ever, or its listing, kept
    /// in sight and read only, nothing typed reaching the session gone; under it a bar with
    /// the failure and the failure card's ways out, or the countdown to the next attempt and
    /// its Cancel.
    fn dropped_page<'a>(
        &'a self,
        tab: &'a Tab,
        error: &'a UiError,
        focused: bool,
    ) -> Element<'a, Message> {
        let kept = match tab.files.as_deref() {
            Some(pane) => self.files_page(tab, pane, false),
            None => self.searchable_terminal(tab, self.app.dialog.is_none() && focused),
        };
        let bar = match tab.retry {
            Some(retry) => countdown_bar(tab.id, retry),
            None => column![
                text(texts::error(error)).style(text::danger),
                self.failure_actions(tab, error)
                    .wrap()
                    .vertical_spacing(spacing::SM),
            ]
            .spacing(spacing::SM)
            .padding(spacing::MD)
            .into(),
        };
        column![container(kept).height(Length::Fill), bar].into()
    }

    /// The line naming the tab an RDP certificate question belongs to, in a tab's own
    /// window, as the C# `RdpTrustPromptOwner` says it: the window's title is built out of
    /// the tab's name, so the tab alone is named.
    fn certificate_owner(&self, tab: &Tab) -> Option<String> {
        (tab.purpose == Purpose::Rdp
            && tab.asks_about_certificate()
            && self.app.is_floating(tab.id))
        .then(|| fl!("ui-certificate-owner-tab", tab = tab.display_title()))
    }

    /// A failed session's card: the error, and its ways out.
    fn failure_card<'a>(&'a self, tab: &'a Tab, error: &'a UiError) -> Element<'a, Message> {
        center(card(
            column![
                text(fl!("ui-session-failed-title")).size(font_size::TITLE),
                text(texts::error(error)),
                // Buttons go to the next line whole when a translation is long, never cut.
                self.failure_actions(tab, error)
                    .wrap()
                    .vertical_spacing(spacing::SM),
            ]
            .spacing(spacing::SM),
        ))
        .into()
    }

    /// The ways out of a failed session: those of [`Self::session_actions`], and the way
    /// past a changed key.
    fn failure_actions<'a>(
        &'a self,
        tab: &'a Tab,
        error: &'a UiError,
    ) -> iced::widget::Row<'a, Message> {
        let mut actions = self.session_actions(tab);
        // A changed key's way out is deliberate, never part of the connection: the old key
        // is forgotten, and the new one asked about as on a first contact. An RDP
        // certificate is routine to change (Windows renews its own every six months); an SSH
        // key, as the C# Heimdall warns, may be an interception.
        let forget = match error {
            UiError::HostKeyChanged { target: None, .. }
                if tab.purpose == Purpose::Rdp || matches!(tab.profile, TabProfile::Ftp(_)) =>
            {
                Some(fl!("ui-session-forget-server-button"))
            }
            UiError::HostKeyChanged {
                target: Some(_), ..
            } => Some(fl!("ui-session-accept-new-key-button")),
            // A certificate the user trusted, no longer valid: forgotten, its replacement is
            // asked about.
            UiError::PinnedCertificateInvalid { .. } => {
                Some(fl!("ui-session-forget-server-button"))
            }
            _ => None,
        };
        if let Some(label) = forget {
            actions = actions.push(
                button(text(label))
                    .style(styles::danger)
                    .on_press(Message::App(AppMessage::ForgetServer(tab.id))),
            );
        }
        actions
    }

    /// What an ended or failed session offers, as the C# Heimdall's card: Reconnect when it
    /// can open again, Copy error for a failure, Edit profile, the way out of a failure
    /// that would only repeat, then Close.
    fn session_actions(&self, tab: &Tab) -> iced::widget::Row<'_, Message> {
        let mut actions = row![].spacing(spacing::SM).align_y(iced::Alignment::Center);
        if self.app.can_reconnect(tab) {
            actions = actions.push(
                button(action_label(fl!("ui-session-reconnect-button")))
                    .style(styles::primary)
                    .on_press(Message::App(AppMessage::ReconnectTab(tab.id))),
            );
        }
        if matches!(
            &tab.phase,
            Phase::Failed(error) if !matches!(error, UiError::Cancelled | UiError::CertificateRefused)
        ) {
            actions = actions
                .push(
                    button(action_label(fl!("ui-session-copy-error-button")))
                        .style(styles::secondary)
                        .on_press(Message::CopyError(tab.id)),
                )
                .push(
                    tooltip(
                        button(action_label(fl!("ui-session-copy-anonymous-button")))
                            .style(styles::secondary)
                            .on_press(Message::CopyAnonymousError(tab.id)),
                        text(fl!("ui-error-report-anonymous-hint")).size(font_size::CAPTION),
                        tooltip::Position::Bottom,
                    )
                    .style(container::rounded_box),
                );
        }
        if let Some(profile) = self
            .app
            .tab_profile(tab)
            .filter(|profile| self.app.can_edit(&profile.id))
        {
            actions = actions.push(
                button(action_label(fl!("ui-session-edit-profile-button")))
                    .style(styles::secondary)
                    .on_press(Message::App(AppMessage::EditProfile(profile.id))),
            );
        }
        actions.push(
            button(action_label(fl!("ui-session-close-button")))
                .style(styles::secondary)
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
            .style(styles::text_input)
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
            stabilizing: pane.stabilizing_until().is_some(),
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
    fn desktop<'a>(&self, tab: &Tab, pane: &'a DesktopPane, focused: bool) -> Element<'a, Message> {
        let fit = self.fits(tab);
        let view = DesktopView::new(pane, tab.id, Message::App)
            .on_release(Message::ContentRelease)
            .interactive(self.takes_keys(tab, focused))
            .fit(fit)
            .density(self.density_of(tab.id));
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
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .text_size(font_size::CAPTION);
        let send_keys = pick_list(
            SpecialKeys::ALL.map(KeysChoice).to_vec(),
            None::<KeysChoice>,
            move |KeysChoice(keys)| Message::App(AppMessage::SendKeys { tab: tab_id, keys }),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .placeholder(fl!("ui-desktop-send-keys"))
        .text_size(font_size::CAPTION);
        let fullscreen = button(
            text(if self.fullscreen {
                fl!("ui-desktop-exit-fullscreen")
            } else {
                fl!("ui-desktop-fullscreen")
            })
            .size(font_size::CAPTION),
        )
        .style(styles::secondary)
        .on_press(Message::ToggleFullscreen);
        let disconnect = tooltip(
            button(text(fl!("ui-desktop-disconnect")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::DisconnectDesktop(tab_id))),
            text(fl!("ui-desktop-disconnect-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let bar = row![
            // Beside it: below, it would cover the menu's first entry.
            tooltip(
                send_keys,
                text(fl!("ui-desktop-send-keys-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Right,
            )
            .style(container::rounded_box),
            // The C# Send keys menu's "Keyboard shortcuts...".
            button(text(fl!("ui-desktop-shortcuts")).size(font_size::CAPTION))
                .style(styles::subtle)
                .on_press(Message::App(AppMessage::ShowShortcuts)),
            mode,
        ]
        // In a tab's own window, the main window's full screen is not the desktop's.
        .push((!self.app.is_floating(tab_id)).then_some(fullscreen))
        .push(disconnect)
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let bar = self.desktop_bar_end(tab, pane, bar);
        column![bar, view].spacing(spacing::XS).into()
    }

    /// The end of a desktop's session bar: the resolution menu, the anti-idle badge, saving
    /// the server's files, VNC's clipboard and quality, the desktop's name and VNC's warning.
    fn desktop_bar_end<'a>(
        &self,
        tab: &Tab,
        pane: &'a DesktopPane,
        mut bar: iced::widget::Row<'a, Message>,
    ) -> iced::widget::Row<'a, Message> {
        let tab_id = tab.id;
        // The C# session bar's resolution button: the tab's menu, its tip naming the mode,
        // in the accent colour while a size of its own is kept. The menus are the main
        // window's: none from a tab's own window.
        if let Some(state) = self
            .resolution_state(tab)
            .filter(|_| !self.app.is_floating(tab_id))
        {
            let style = if state.fixed.is_some() {
                styles::primary
            } else {
                styles::secondary
            };
            bar = bar.push(
                tooltip(
                    button(text(fl!("ui-resolution-menu")).size(font_size::CAPTION))
                        .style(style)
                        .on_press(Message::OpenTreeMenu(TreeMenu::Resolution(tab_id))),
                    text(state.tooltip()).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        // The C# status line's countdown while the session settles after connecting, its
        // tip saying how to stop waiting.
        if let Some(seconds) = pane.stabilization_seconds_left(std::time::Instant::now()) {
            bar = bar.push(
                tooltip(
                    text(fl!("ui-desktop-stabilizing", seconds = seconds))
                        .size(font_size::CAPTION)
                        .font(iced::Font {
                            style: iced::font::Style::Italic,
                            ..crate::UI_FONT
                        })
                        .style(text::secondary),
                    text(fl!("ui-desktop-stabilizing-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        // Shown while the session gets anti-idle keys; a click stops them, as the C# badge.
        if self.app.anti_idle_on(tab_id) {
            bar = bar.push(
                tooltip(
                    button(text(fl!("ui-desktop-anti-idle")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press(Message::App(AppMessage::StopAntiIdle(tab_id))),
                    text(fl!("ui-desktop-anti-idle-tooltip")).size(font_size::CAPTION),
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
                    button(text(fl!("ui-desktop-send-clipboard")).size(font_size::CAPTION))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::SendClipboard(tab_id))),
                    text(fl!("ui-desktop-send-clipboard-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Right,
                )
                .style(container::rounded_box),
            );
        }
        if let Some(control) = vnc_quality_control(pane, tab_id) {
            bar = bar.push(control);
        }
        if let TabProfile::Rdp(profile) = &tab.profile {
            bar = bar.extend(redirection_badges(profile));
        }
        if let Some(name) = &pane.desktop_name {
            bar = bar.push(
                text(name.as_str())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        if tab.purpose == Purpose::Vnc {
            // Always in sight: whether the connection is encrypted, its server's certificate
            // trusted, or crosses the network in clear.
            bar = bar.push(match pane.tls {
                Some(version) => text(fl!("ui-session-vnc-encrypted", version = version))
                    .size(font_size::CAPTION)
                    .style(text::success),
                None => text(fl!("ui-session-vnc-unencrypted"))
                    .size(font_size::CAPTION)
                    .style(text::danger),
            });
        }
        bar
    }

    fn question<'a>(&'a self, tab: &'a Tab, prompt: &'a Prompt) -> Element<'a, Message> {
        let id = prompt.question;
        let mut form = Column::new().spacing(spacing::SM);
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
                                .size(font_size::CAPTION),
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
                    .style(styles::secondary)
                    .on_press(Message::Decline(tab.id)),
                button(text(fl!("ui-prompt-submit-button")))
                    .style(styles::primary)
                    .on_press(Message::Submit(tab.id)),
            ]
            .spacing(spacing::SM),
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
        && tab.elevated.is_none()
        && !matches!(tab.purpose, Purpose::Files | Purpose::Rdp | Purpose::Vnc)
        && matches!(tab.phase, Phase::Connected | Phase::Closed { .. })
}

/// The question about an unknown key, as the C# Heimdall asks it: an SSH host's, or, when
/// `certificate` names the profile, an RDP or FTPS server's own certificate, with the other
/// certificates already trusted for the name and the gateways on the way. Either can be
/// trusted for this run only, never recorded, and its fingerprint copied.
fn host_key_card<'a>(
    tab: TabId,
    (host, port): (&'a str, u16),
    (fingerprint, algorithm): (&'a str, Option<String>),
    certificate: Option<(&'a str, Option<&'a CertificateContext>)>,
    owner: Option<String>,
) -> Element<'a, Message> {
    let port = port.to_string();
    let (heading, body, label, [reject, once, accept]) = match certificate {
        Some((name, context)) => (
            fl!("ui-certificate-title"),
            certificate_body(name, host, &port, context).push(owner.map(text)),
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
            ))]
            // The key's algorithm, as the C# prompt's row.
            .push(algorithm.map(|algorithm| {
                text(fl!("ui-hostkey-algorithm", algorithm = algorithm)).font(iced::Font::MONOSPACE)
            })),
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
            // As the C# `HostKeyPromptDialog`: the warning's icon beside its title.
            dialog_parts::header(Severity::Warning, dialog_parts::title(heading)),
            body,
            row![
                text(label).font(iced::Font::MONOSPACE).width(Length::Fill),
                button(text(fl!("ui-hostkey-copy-fingerprint-button")).size(font_size::CAPTION))
                    .style(styles::secondary)
                    .on_press(Message::App(AppMessage::CopyHostKeyFingerprint(tab))),
            ]
            .spacing(spacing::SM)
            .align_y(iced::alignment::Vertical::Center),
            row![
                dialog_parts::action(reject, styles::secondary).on_press(Message::App(
                    AppMessage::HostKeyDecision { tab, accept: false }
                )),
                dialog_parts::action(once, styles::secondary)
                    .on_press(Message::App(AppMessage::HostKeyTrustOnce(tab))),
                dialog_parts::action(accept, styles::primary).on_press(Message::App(
                    AppMessage::HostKeyDecision { tab, accept: true }
                )),
            ]
            .spacing(spacing::SM)
            .wrap(),
        ]
        .spacing(spacing::SM),
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
    .spacing(spacing::SM);
    let Some(context) = context else {
        return body;
    };
    if let Some(subject) = &context.subject {
        body = body.push(
            text(fl!(
                "ui-certificate-subject",
                subject = server_text(subject)
            ))
            .font(iced::Font::MONOSPACE),
        );
    }
    if let Some(details) = &context.details {
        body = body.push(certificate_details(details, std::time::SystemTime::now()));
    }
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

/// What the FTPS and VNC certificate question says under the subject, as the C# prompt: the
/// issuer, when the certificate holds, marked when `now` is outside it, and why the system
/// did not vouch for it, when it was asked. A certificate renewed on a key trusted, an RDP
/// one's included, says so first, with when the certificate on record held, when recorded.
fn certificate_details<'a>(
    details: &heimdall_app::CertificateDetails,
    now: std::time::SystemTime,
) -> iced::widget::Column<'a, Message> {
    let period = match details.validity.period(now) {
        heimdall_rdp::ValidityPeriod::Current => "current",
        heimdall_rdp::ValidityPeriod::Expired => "expired",
        heimdall_rdp::ValidityPeriod::NotYetValid => "future",
    };
    let mut body = Column::new().spacing(spacing::SM);
    if let Some(renewed) = &details.renewal {
        body = body.push(text(fl!("ui-certificate-renewed")));
        if let Some(recorded) = renewed.recorded {
            body = body.push(text(fl!(
                "ui-certificate-renewed-previous",
                from = crate::files_view::modified_text(recorded.not_before),
                until = crate::files_view::modified_text(recorded.not_after)
            )));
        }
    }
    body.push(
        text(fl!(
            "ui-certificate-issuer",
            issuer = server_text(&details.issuer)
        ))
        .font(iced::Font::MONOSPACE),
    )
    .push(text(fl!(
        "ui-certificate-validity",
        from = crate::files_view::modified_text(details.validity.not_before),
        until = crate::files_view::modified_text(details.validity.not_after),
        period = period
    )))
    .push(details.issue.map(|issue| {
        text(fl!(
            "ui-certificate-validation-issue",
            issue = texts::validation_issue(issue)
        ))
    }))
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(spacing::LG)
        .max_width(CARD_WIDTH)
        .style(styles::dialog)
        .into()
}

/// A session action's label, kept on one line: a card moves the whole button instead.
fn action_label<'a>(label: String) -> iced::widget::Text<'a> {
    text(label).wrapping(text::Wrapping::None)
}

/// A card for a table, wider than a form's.
fn wide_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(spacing::LG)
        .max_width(WIDE_CARD_WIDTH)
        .style(styles::dialog)
        .into()
}

/// The profile form's card, as the C# server dialog's window: as wide as it, and as high
/// as it or as `height`, the room the window leaves.
fn form_card<'a>(content: impl Into<Element<'a, Message>>, height: f32) -> Element<'a, Message> {
    container(content)
        .padding(spacing::LG)
        .max_width(FORM_SIZE.width)
        .height(FORM_SIZE.height.min(height).max(0.0))
        .style(styles::dialog)
        .into()
}

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

/// The tabs of the Settings page.
fn settings_tabs<'a>(shown: SettingsTab) -> Element<'a, Message> {
    tab_strip(
        SettingsTab::ALL,
        shown,
        SettingsTab::label,
        Message::SettingsTab,
        |_| 0,
    )
    .into()
}

/// Headers of tabs, as the C# `TabControl` of `ThemedTabItemStyle`: each its `label` over a
/// line, the one `shown` a card, its name semi-bold over the accent's line, and how many
/// fields its `errors` are on it in a badge beside its name; a click sends `show`.
fn tab_strip<'a, T: Copy + PartialEq>(
    tabs: impl IntoIterator<Item = T>,
    shown: T,
    label: fn(T) -> String,
    show: fn(T) -> Message,
    errors: impl Fn(T) -> usize,
) -> iced::widget::Row<'a, Message> {
    tabs.into_iter().fold(row![].spacing(TAB_GAP), |tabs, tab| {
        let selected = tab == shown;
        let name = text(label(tab));
        let name = if selected {
            name.font(styles::SEMIBOLD)
        } else {
            name
        };
        let count = errors(tab);
        let header = row![name]
            .push((count > 0).then(|| error_badge(count)))
            .spacing(ERROR_BADGE_GAP)
            .align_y(iced::Alignment::Center);
        tabs.push(
            button(
                column![
                    header,
                    container(iced::widget::space())
                        .width(Length::Fill)
                        .height(TAB_UNDERLINE)
                        .style(styles::underline(selected)),
                ]
                .spacing(spacing::XS)
                .width(Length::Shrink),
            )
            .padding(TAB_PADDING)
            .style(styles::tab(selected))
            .on_press(show(tab)),
        )
    })
}

/// How many fields of a tab are to fix, as the C# server dialog's badge on its header: the
/// count, small, on the error colour, in a pill at least as wide as it is high.
fn error_badge<'a>(count: usize) -> Element<'a, Message> {
    container(
        column![
            text(count.to_string())
                .size(font_size::BADGE)
                .wrapping(text::Wrapping::None),
            // Never narrower than it is high, wider when its count is.
            iced::widget::space().width(ERROR_BADGE_SIDE - 2.0 * ERROR_BADGE_PADDING_X),
        ]
        .align_x(iced::Alignment::Center),
    )
    .center_y(ERROR_BADGE_SIDE)
    .padding([0.0, ERROR_BADGE_PADDING_X])
    .style(styles::error_badge)
    .into()
}

/// The open dialog of "Import Sessions", held by the window, then the file read; a `.rdp`
/// file goes to the `.rdp` import, as the C# sends it. Nothing when no file is picked.
fn pick_sessions_file(main: Option<window::Id>) -> Task<Message> {
    main_window_task(main).then(|id| {
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
fn pick_rdp(main: Option<window::Id>) -> Task<Message> {
    let (title, filter) = (fl!("ui-rdp-title"), fl!("ui-rdp-filter"));
    main_window_task(main).then(move |id| {
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
                button(text(fl!("ui-desktop-save-files")).size(font_size::CAPTION))
                    .style(styles::secondary)
                    .on_press(Message::App(AppMessage::SaveRemoteFiles(tab_id))),
                text(fl!("ui-desktop-save-files-tooltip")).size(font_size::CAPTION),
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
            text(fl!("ui-desktop-saving-files", saved = saved, total = total))
                .size(font_size::CAPTION),
            button(text(fl!("ui-desktop-save-files-cancel")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::CancelSave(tab_id))),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center)
        .into(),
    )
}

/// A VNC desktop's "Quality" menu, beside its clipboard as in the C# toolbar: how its
/// server trades the picture for bandwidth, for this tab only.
fn vnc_quality_control(pane: &DesktopPane, tab_id: TabId) -> Option<Element<'_, Message>> {
    let quality = pane.vnc_quality()?;
    let choices = pick_list(
        VncQuality::ALL.map(QualityChoice).to_vec(),
        Some(QualityChoice(quality)),
        move |QualityChoice(quality)| {
            Message::App(AppMessage::VncQuality {
                tab: tab_id,
                quality,
            })
        },
    )
    .style(styles::pick_list)
    .menu_style(styles::menu)
    .text_size(font_size::CAPTION);
    Some(
        tooltip(
            choices,
            text(fl!("ui-session-vnc-quality")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into(),
    )
}

/// The folder dialog of "Save copied files...", held by the window: the folder picked
/// goes to the session of `tab`.
fn pick_save_folder(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    type PickFolder =
        std::pin::Pin<Box<dyn std::future::Future<Output = Option<rfd::FileHandle>> + Send>>;
    let title = fl!("ui-desktop-save-files");
    main_window_task(main).then(move |id| {
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
///
/// The image is read through the system's own drawing calls, sized from the picture, and
/// never copied as the memory block the clipboard names: for a bitmap the system made from
/// another program's format (a picture copied from a browser), that block's stated size can
/// run past what can be read, which ended the application.
fn clipboard_image() -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        use clipboard_win::{Clipboard, formats, raw};
        let _open = Clipboard::new_attempts(CLIPBOARD_ATTEMPTS).ok()?;
        // The block's size only, nothing read from it: an image too large is not offered.
        let size = raw::size(formats::CF_DIB)?.get();
        if size > heimdall_rdp::MAX_IMAGE_BYTES {
            return None;
        }
        let mut file = Vec::new();
        raw::get_bitmap(&mut file).ok()?;
        dib_of_bitmap_file(&file)
            .filter(|image| image.len() <= heimdall_rdp::MAX_IMAGE_BYTES)
            .map(<[u8]>::to_vec)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Bytes of a bitmap file's header, before its device-independent bitmap.
#[cfg(any(windows, test))]
const BITMAP_FILE_HEADER_BYTES: usize = 14;

/// The two bytes a bitmap file starts with, "BM".
#[cfg(any(windows, test))]
const BITMAP_FILE_MAGIC: [u8; 2] = *b"BM";

/// The device-independent bitmap a bitmap file holds: the file without its header. `None`
/// for anything that is not a bitmap file, or holds nothing after its header.
#[cfg(any(windows, test))]
fn dib_of_bitmap_file(file: &[u8]) -> Option<&[u8]> {
    let image = file
        .strip_prefix(&BITMAP_FILE_MAGIC)
        .filter(|_| file.len() > BITMAP_FILE_HEADER_BYTES)?;
    Some(&image[BITMAP_FILE_HEADER_BYTES - BITMAP_FILE_MAGIC.len()..])
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

/// Puts `paths`, files of this computer, on the clipboard as copied files in place of what
/// it held, as Explorer's Copy puts them: the local file browser's "Copy". Windows only:
/// elsewhere Heimdall holds them itself.
fn write_file_list(paths: &[std::path::PathBuf]) {
    #[cfg(windows)]
    {
        use clipboard_win::{Clipboard, options, raw};
        // A path the list cannot carry, not Unicode, is left out.
        let listed: Vec<&str> = paths.iter().filter_map(|path| path.to_str()).collect();
        if listed.is_empty() {
            return;
        }
        // Another program may hold the clipboard a moment: asked again a little later.
        for _ in 0..CLIPBOARD_ATTEMPTS {
            if let Ok(_open) = Clipboard::new() {
                if let Err(error) = raw::set_file_list_with(&listed, options::DoClear) {
                    log::warn!("the copied files did not reach the clipboard: {error}");
                }
                return;
            }
            std::thread::sleep(CLIPBOARD_RETRY);
        }
        log::warn!("the copied files did not reach the clipboard: it stayed held");
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
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
fn pick_uploads(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-files-menu-upload-here");
    main_window_task(main).then(move |id| {
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
fn pick_known_hosts(main: Option<window::Id>) -> Task<Message> {
    main_window_task(main).then(|id| {
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
fn pick_openssh(main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-openssh-title");
    main_window_task(main).then(move |id| {
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
fn save_export(document: String, count: usize, main: Option<window::Id>) -> Task<Message> {
    let (title, filter) = (fl!("ui-dialog-export-title"), fl!("ui-export-filter-json"));
    main_window_task(main).then(move |id| {
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
    let detail =
        |detail: &str| text(fl!("ui-dialog-detail", detail = detail)).size(font_size::CAPTION);
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
        Dialog::StoreUnreadable { detail: technical } => (
            fl!("ui-dialog-store-title"),
            vec![text(fl!("ui-dialog-store-body")), detail(technical)],
        ),
        Dialog::StoreError { detail: technical } => (
            fl!("ui-dialog-save-failed-title"),
            vec![text(fl!("ui-dialog-save-failed-body", detail = technical))],
        ),
        Dialog::StoreChanged { detail: technical } => (
            fl!("ui-dialog-store-changed-title"),
            vec![text(fl!("ui-dialog-store-changed-body")), detail(technical)],
        ),
        other => {
            let (title, lines) = crate::rdp_view::report_lines(other)
                .map(|lines| (fl!("ui-rdp-title"), lines))
                .or_else(|| crate::sessions_view::report_lines(other))
                .or_else(|| crate::hostkeys_view::report_lines(other))
                .or_else(|| crate::citrix_import_view::report_lines(other))
                .unwrap_or_default();
            (title, lines.into_iter().map(text).collect())
        }
    };
    let severity = match dialog {
        Dialog::ExportFailed { .. }
        | Dialog::ImportFailed { .. }
        | Dialog::PasswordSaveFailed { .. }
        | Dialog::StoreUnreadable { .. }
        | Dialog::StoreError { .. } => Severity::Error,
        Dialog::StoreChanged { .. } => Severity::Warning,
        _ => Severity::Info,
    };
    dialog_parts::message(
        severity,
        title,
        Column::with_children(lines.into_iter().map(Element::from)).spacing(spacing::SM),
        dialog_parts::buttons([ok]),
    )
}

/// The preview of an import, a table of what the file gives to choose from.
fn import_preview(dialog: &Dialog) -> Element<'_, Message> {
    match dialog {
        Dialog::SessionsPreview(preview) => crate::sessions_view::preview(preview),
        Dialog::RdpPreview(preview) => crate::rdp_view::preview(preview),
        Dialog::HostKeysPreview(preview) => crate::hostkeys_view::preview(preview),
        Dialog::ConfirmImportFile(_) | Dialog::ConfirmCitrixImport(_) => {
            let (title, body, action) = match dialog {
                Dialog::ConfirmImportFile(pending) => crate::file_import_view::question(pending),
                Dialog::ConfirmCitrixImport(scan) => crate::citrix_import_view::question(scan),
                _ => return column![].into(),
            };
            // As the C# `ShowConfirm` before an import reads the file.
            dialog_parts::question(Severity::Info, title, body, action)
        }
        _ => column![].into(),
    }
}

/// The report of an import, as the C# message after it: what was added, updated and left as
/// it was, the profiles left out and why, the settings dropped, the host keys carried.
fn import_report<'a>(
    summary: &'a heimdall_app::ImportSummary,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let mut content = column![dialog_parts::body(fl!(
        "ui-dialog-import-counts",
        added = summary.merged.added,
        updated = summary.merged.updated,
        unchanged = summary.merged.unchanged
    ))]
    .spacing(spacing::SM);
    // The file's gateways on a line of their own, as the C# summary says them, then what to
    // do about the references none resolves (`ProfileImportService.cs:446-476`).
    let gateways = summary.gateways;
    if gateways.created + gateways.merged + gateways.orphans > 0 {
        content = content.push(text(fl!(
            "ui-dialog-import-gateways",
            created = gateways.created,
            merged = gateways.merged,
            orphans = gateways.orphans
        )));
    }
    if gateways.orphans > 0 {
        content = content.push(
            text(fl!("ui-dialog-import-gateways-orphans-action"))
                .size(font_size::CAPTION)
                .style(text::warning),
        );
    }
    if !summary.skipped.is_empty() {
        let skipped = summary.skipped.iter().fold(
            Column::new().spacing(spacing::XS),
            |list, (name, reason)| {
                list.push(
                    text(fl!(
                        "ui-dialog-import-skipped-item",
                        name = server_text(name),
                        reason = texts::skip_reason(reason)
                    ))
                    .size(font_size::CAPTION),
                )
            },
        );
        content = content
            .push(text(fl!("ui-dialog-import-skipped")))
            .push(container(styles::scroll(skipped)).max_height(SKIPPED_LIST_HEIGHT));
    }
    if !summary.dropped.is_empty() {
        let dropped = summary.dropped.iter().fold(
            Column::new().spacing(spacing::XS),
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
                    .size(font_size::CAPTION),
                )
            },
        );
        content = content
            .push(text(fl!("ui-dialog-import-dropped")))
            .push(container(styles::scroll(dropped)).max_height(SKIPPED_LIST_HEIGHT));
    }
    match &summary.host_keys {
        Some(Ok(carried)) if carried.keys + carried.pins > 0 => {
            content = content.push(
                text(fl!(
                    "ui-dialog-import-host-keys",
                    keys = carried.keys,
                    pins = carried.pins
                ))
                .size(font_size::CAPTION),
            );
        }
        Some(Err(detail)) => {
            content = content.push(
                text(fl!(
                    "ui-dialog-import-host-keys-failed",
                    detail = detail.as_str()
                ))
                .size(font_size::CAPTION)
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
        content = content.push(text(line).size(font_size::CAPTION));
    }
    // As the C# `ShowInfo` after an import.
    dialog_parts::message(
        Severity::Info,
        fl!("ui-dialog-import-title"),
        content,
        dialog_parts::buttons([ok]),
    )
}

/// What the dialogs show that the window holds.
struct Forms<'a> {
    /// The vault dialog's fields.
    vault: &'a [Zeroizing<String>; 3],
    /// The profile form's password.
    profile_password: &'a str,
    /// The bulk password dialog's fields.
    bulk_password: &'a [Zeroizing<String>; 2],
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
    /// This computer's screens, as last listed for the RDP profile form.
    monitors: &'a [crate::rdp_options::Monitor],
    /// The profile form's tab shown.
    profile_tab: ProfileTab,
    /// Whether a save of the profile form was refused, its tabs counting what to fix.
    profile_refused: bool,
    /// The settings' wait after connecting, in milliseconds: what an RDP profile leaving
    /// its own empty takes, said in the field.
    rdp_resize_delay: u32,
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

impl PasswordStore {
    /// Why no password can be saved now, as the profile form says it; `None` when one can.
    fn blocked(self) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::VaultLocked => Some(fl!("ui-profile-password-locked")),
            Self::None => Some(fl!("ui-profile-password-no-store")),
        }
    }
}

/// The password of a profile, as the C# editor shows it: an empty field whatever is saved,
/// "Password saved" and a button to clear it when one is.
fn password_field<'a>(draft: &ProfileDraft, forms: &Forms<'a>) -> Element<'a, Message> {
    let mut input = text_input("", forms.profile_password)
        .style(styles::text_input)
        .id(password_field_id())
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        input = input
            .on_input(Message::ProfilePassword)
            .on_submit(Message::SaveProfileForm);
    }
    let mut field =
        column![dialog_parts::label(fl!("ui-profile-field-password")), input].spacing(spacing::XS);
    match forms.passwords {
        PasswordStore::Ready if draft.password_saved => {
            field = field.push(
                row![
                    dialog_parts::hint(fl!("ui-profile-password-saved")),
                    tooltip(
                        button(text(fl!("ui-profile-password-clear")).size(font_size::CAPTION))
                            .style(styles::subtle)
                            .on_press(Message::App(AppMessage::ClearPassword)),
                        text(fl!("ui-profile-password-clear-tooltip")).size(font_size::CAPTION),
                        tooltip::Position::Top,
                    )
                    .style(container::rounded_box),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            );
        }
        PasswordStore::Ready => {}
        PasswordStore::VaultLocked => {
            field = field.push(dialog_parts::hint(fl!("ui-profile-password-locked")));
        }
        PasswordStore::None => {
            field = field.push(dialog_parts::hint(fl!("ui-profile-password-no-store")));
        }
    }
    field.into()
}

/// The profile form's page, scrolled under its tabs.
#[must_use]
pub fn profile_page_id() -> iced::widget::Id {
    iced::widget::Id::from("profile-page")
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
        DraftProtocol::Citrix => fl!("ui-profile-protocol-citrix-name"),
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
        DraftProtocol::Citrix => fl!("ui-profile-protocol-citrix-desc"),
        DraftProtocol::Local => fl!("ui-profile-protocol-local-desc"),
    }
}

/// The first step of a new session, as in the C# dialog: a card per protocol, four to a
/// row, each its icon in the protocol's colour, its name and what it is.
fn protocol_picker<'a>() -> Element<'a, Message> {
    let card = |protocol: DraftProtocol| {
        let kind = protocol_kind(protocol);
        button(
            column![
                icons::icon(Icon::of(kind), Tint::Protocol(kind), PICKER_ICON_SIDE),
                text(protocol_name(protocol)).font(styles::SEMIBOLD),
                text(protocol_description(protocol))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .align_x(iced::alignment::Horizontal::Center),
            ]
            .spacing(spacing::XS)
            .align_x(iced::Alignment::Center)
            .width(Length::Fill),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::secondary)
        .on_press(Message::App(AppMessage::ChooseProtocol(protocol)))
    };
    let cards = DraftProtocol::ALL.chunks(PICKER_COLUMNS).fold(
        Column::new().spacing(spacing::SM),
        |cards, chunk| {
            let mut line = row![].spacing(spacing::SM);
            for protocol in chunk {
                line = line.push(card(*protocol));
            }
            // A last row short of cards keeps their width.
            for _ in chunk.len()..PICKER_COLUMNS {
                line = line.push(iced::widget::space::horizontal());
            }
            cards.push(line)
        },
    );
    column![
        text(fl!("ui-profile-new-title"))
            .size(font_size::TITLE)
            .font(styles::SEMIBOLD),
        section(
            fl!("ui-profile-protocol-picker-title"),
            Some(fl!("ui-profile-protocol-picker-desc"))
        ),
        styles::scroll(cards).height(Length::Fill),
        dialog_parts::buttons([dialog_parts::cancel()]),
    ]
    .spacing(spacing::MD)
    .height(Length::Fill)
    .into()
}

/// The kind of profile `protocol` makes, whose icon and colour it takes.
fn protocol_kind(protocol: DraftProtocol) -> heimdall_app::ProfileKind {
    use heimdall_app::ProfileKind;
    match protocol {
        DraftProtocol::Rdp => ProfileKind::Rdp,
        DraftProtocol::Ssh => ProfileKind::Ssh,
        DraftProtocol::Sftp => ProfileKind::Sftp,
        DraftProtocol::WinRm => ProfileKind::WinRm,
        DraftProtocol::Vnc => ProfileKind::Vnc,
        DraftProtocol::Telnet => ProfileKind::Telnet,
        DraftProtocol::Ftp => ProfileKind::Ftp,
        DraftProtocol::Citrix => ProfileKind::Citrix,
        DraftProtocol::Local => ProfileKind::Local,
    }
}

/// A section of the form: its title, and its description when it has one.
fn section<'a>(title: String, description: Option<String>) -> Element<'a, Message> {
    dialog_parts::section(title, description)
}

/// A text field of the form: its label, then the box. Enter saves.
fn form_field(draft: &ProfileDraft, field: ProfileField) -> Element<'_, Message> {
    labelled_field(draft, field, None)
}

/// A text field of the form, with `shown` in its empty box in place of its own placeholder
/// when given.
fn labelled_field(
    draft: &ProfileDraft,
    field: ProfileField,
    shown: Option<String>,
) -> Element<'_, Message> {
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
                DraftProtocol::Ftp => fl!("ui-profile-port-ftp"),
                // Not shown: a local shell and a Citrix application have no port.
                DraftProtocol::Telnet | DraftProtocol::Local | DraftProtocol::Citrix => {
                    fl!("ui-profile-port-telnet")
                }
            },
            draft.default_port().to_string(),
        ),
        ProfileField::StoreFrontUrl => (fl!("ui-profile-field-storefront-url"), String::new()),
        ProfileField::AppName => (fl!("ui-profile-field-app-name"), String::new()),
        ProfileField::IcaFile => (fl!("ui-profile-field-ica-file"), String::new()),
        ProfileField::Username => (
            fl!("ui-profile-field-username"),
            match draft.protocol {
                DraftProtocol::Rdp => fl!("ui-profile-username-rdp-placeholder"),
                // Few VNC servers ask for one: said what it is for.
                DraftProtocol::Vnc => fl!("ui-profile-username-vnc-placeholder"),
                _ => fl!("ui-profile-optional"),
            },
        ),
        ProfileField::Domain => (
            fl!("ui-profile-field-domain"),
            fl!("ui-profile-domain-placeholder"),
        ),
        ProfileField::KeyPath => (fl!("ui-profile-field-key"), fl!("ui-profile-optional")),
        ProfileField::FixedWidth => (fl!("ui-profile-resolution-width"), String::new()),
        ProfileField::FixedHeight => (fl!("ui-profile-resolution-height"), String::new()),
        ProfileField::ResizeDelay => (fl!("ui-profile-resize-delay"), String::new()),
        ProfileField::VaultEntry => (
            fl!("ui-profile-field-vault-entry"),
            fl!("ui-profile-vault-entry-placeholder"),
        ),
        ProfileField::SocksPort => (fl!("ui-profile-socks-port"), PORT_OFF.to_string()),
        ProfileField::RemoteBindPort => (fl!("ui-profile-remote-bind-port"), PORT_OFF.to_string()),
        ProfileField::RemoteLocalPort => {
            (fl!("ui-profile-remote-local-port"), PORT_OFF.to_string())
        }
        ProfileField::RdGateway => (
            fl!("ui-profile-field-rd-gateway"),
            fl!("ui-profile-rd-gateway-placeholder"),
        ),
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
    let placeholder = shown.unwrap_or(placeholder);
    column![
        dialog_parts::label(label),
        text_input(&placeholder, draft.value(field))
            .style(styles::text_input)
            .id(profile_field_id(field))
            .on_input(move |value| Message::App(AppMessage::ProfileField { field, value }))
            .on_submit(Message::SaveProfileForm),
    ]
    .spacing(spacing::XS)
    .into()
}

/// The SSH key field, with the C# "Browse..." button beside it.
fn key_field(draft: &ProfileDraft) -> Element<'_, Message> {
    let field = ProfileField::KeyPath;
    column![
        dialog_parts::label(fl!("ui-profile-field-key")),
        row![
            text_input(&fl!("ui-profile-optional"), draft.value(field))
                .style(styles::text_input)
                .id(profile_field_id(field))
                .on_input(move |value| Message::App(AppMessage::ProfileField { field, value }))
                .on_submit(Message::SaveProfileForm),
            button(text(fl!("ui-profile-browse-button")))
                .style(styles::secondary)
                .on_press(Message::BrowseKeyFile),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(spacing::XS)
    .into()
}

/// The open dialog of the profile form's SSH key, held by the window, in `~/.ssh`: every
/// file first, since an OpenSSH key has no extension, then the C# `.ppk` and `.pem`. The
/// path picked fills the field; nothing when none is picked.
fn pick_key_file(main: Option<window::Id>) -> Task<Message> {
    let dialog = || {
        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title(fl!("ui-profile-browse-key-title"))
            .add_filter(fl!("ui-profile-browse-key-all"), &["*"])
            .add_filter(fl!("ui-profile-browse-key-ppk"), &["ppk"])
            .add_filter(fl!("ui-profile-browse-key-pem"), &["pem"]);
        if let Some(folder) = paths::home_dir()
            .map(|home| home.join(paths::OPENSSH_FOLDER))
            .filter(|folder| folder.is_dir())
        {
            dialog = dialog.set_directory(folder);
        }
        dialog
    };
    main_window_task(main).then(move |id| {
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
        .style(styles::checkbox)
        .label(label)
        .on_toggle(move |on| Message::App(AppMessage::ProfileToggle { toggle, on }))
        .into()
}

/// The C# hint under `toggle`, for the boxes the C# dialog explains.
fn toggle_hint(toggle: ProfileToggle) -> Option<String> {
    match toggle {
        ProfileToggle::UseSsl => Some(fl!("ui-profile-use-ssl-hint")),
        ProfileToggle::SkipCertificateCheck => Some(fl!("ui-profile-skip-cert-hint")),
        ProfileToggle::Sso => Some(fl!("ui-profile-toggle-sso-hint")),
        ProfileToggle::X11Forwarding => Some(fl!("ui-profile-toggle-x11-hint")),
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
        ProfileToggle::RequireTls => fl!("ui-profile-toggle-require-tls"),
        ProfileToggle::DirectConnection => fl!("ui-profile-direct-connect"),
        ProfileToggle::AdminSession => fl!("ui-profile-toggle-admin"),
        ProfileToggle::ForwardAgent => fl!("ui-profile-toggle-forward-agent"),
        ProfileToggle::Compression => fl!("ui-profile-toggle-compression"),
        ProfileToggle::X11Forwarding => fl!("ui-profile-toggle-x11"),
        ProfileToggle::LegacyAlgorithms => fl!("ui-profile-toggle-legacy-algorithms"),
        ProfileToggle::Passive => fl!("ui-profile-toggle-passive"),
        ProfileToggle::Tls => fl!("ui-profile-toggle-ftps"),
        ProfileToggle::Seamless => fl!("ui-profile-toggle-seamless"),
        ProfileToggle::Sso => fl!("ui-profile-toggle-sso"),
        ProfileToggle::Favorite => fl!("ui-profile-toggle-favorite"),
        ProfileToggle::RunAsAdministrator => fl!("ui-profile-local-run-as-admin"),
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
/// gateway chosen, added or edited here; for RDP, then the RD Gateway.
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
    .spacing(spacing::MD);
    if gateways.is_empty() {
        section_column = section_column
            .push(dialog_parts::hint(fl!("ui-gateway-list-empty")))
            .push(dialog_parts::hint(fl!("ui-gateway-empty-hint")));
    }
    let routed = draft.routed_gateway();
    if direct {
        section_column =
            section_column.push(dialog_parts::hint(fl!("ui-profile-gateway-direct-hint")));
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
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .width(Length::Fill),
                button(text(fl!("ui-gateway-add")))
                    .style(styles::secondary)
                    .on_press(Message::App(AppMessage::NewGateway)),
            ]
            .spacing(spacing::SM),
        );
        section_column = section_column.push(dialog_parts::hint(if routed.is_some() {
            fl!("ui-profile-gateway-explain-tunnel")
        } else {
            fl!("ui-profile-gateway-explain-direct")
        }));
    }
    if let Some(id) = routed.filter(|id| gateways.iter().any(|known| known.id == *id)) {
        section_column = section_column.push(
            button(text(fl!("ui-profile-edit-gateway")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::EditGateway(id))),
        );
    }
    // Said where the SSL box was, as the C# dialog says it; HTTPS asked for, then a gateway
    // chosen, is said to be off.
    if draft.protocol == DraftProtocol::WinRm && draft.routed_gateway().is_some() {
        section_column =
            section_column.push(dialog_parts::hint(fl!("ui-profile-winrm-gateway-http")));
        if draft.is_on(ProfileToggle::UseSsl) {
            section_column = section_column.push(
                text(fl!("ui-profile-winrm-https-off-by-gateway"))
                    .size(font_size::CAPTION)
                    .style(text::warning),
            );
        }
    }
    if draft.shows(ProfileField::SocksPort) {
        section_column = forward_cards(draft, section_column);
    }
    // After the SSH gateway, as the C# Network tab asks it.
    if draft.shows(ProfileField::RdGateway) {
        section_column = section_column.push(crate::rdp_options::rd_gateway(
            draft,
            form_field(draft, ProfileField::RdGateway),
        ));
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
        .push(text(listening).size(font_size::CAPTION))
        .push(section(
            fl!("ui-profile-remote-title"),
            Some(fl!("ui-profile-remote-desc")),
        ))
        .push(
            row![
                port_field(ProfileField::RemoteBindPort),
                port_field(ProfileField::RemoteLocalPort),
            ]
            .spacing(spacing::SM),
        )
        .push(dialog_parts::hint(fl!("ui-profile-remote-local-hint")))
        .push(text(route).size(font_size::CAPTION))
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
    let mut form = Column::new().spacing(spacing::SM);
    for field in GATEWAY_FIELDS {
        let label = match field {
            ProfileField::Name => fl!("ui-gateway-field-name"),
            ProfileField::Host => fl!("ui-gateway-field-host"),
            ProfileField::Port => fl!("ui-gateway-field-port"),
            ProfileField::Username => fl!("ui-gateway-field-username"),
            _ => fl!("ui-gateway-field-key"),
        };
        let input = text_input("", draft.value(field))
            .style(styles::text_input)
            .id(gateway_field_id(field))
            .on_input(move |value| Message::App(AppMessage::GatewayField { field, value }))
            .on_submit(Message::SaveGatewayForm);
        // The key file, "Browse..." beside it, as the C# gateway dialog
        // (`GatewayDialog.xaml:81-89`).
        let input: Element<'a, Message> = if field == ProfileField::KeyPath {
            row![
                input,
                crate::browse::browse_button(crate::browse::BrowseTarget::GatewayKey)
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            input.into()
        };
        form = form.push(column![dialog_parts::dialog_label(label), input].spacing(spacing::XS));
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
        form = form.push(dialog_parts::error(texts::draft_error(error)));
    }
    // What is tested is what is saved: not while the test runs.
    let testing = matches!(
        draft.route_test,
        heimdall_app::gateway_draft::RouteTest::Running(_)
    );
    dialog_parts::form(
        dialog_parts::title(title),
        form,
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::confirm(
                fl!("ui-profile-save-button"),
                (!testing).then_some(Message::SaveGatewayForm),
            ),
        ]),
    )
}

/// The gateway's password, as the session form's: an empty field, "Password saved" and
/// Clear, then the C# hint.
fn gateway_password<'a>(draft: &GatewayDraft, forms: &Forms<'a>) -> Column<'a, Message> {
    let mut form = Column::new().spacing(spacing::SM);
    let mut password = text_input("", forms.gateway_password)
        .style(styles::text_input)
        .id(gateway_field_id(ProfileField::Domain))
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        password = password
            .on_input(Message::GatewayPassword)
            .on_submit(Message::SaveGatewayForm);
    }
    form = form.push(
        column![
            dialog_parts::dialog_label(fl!("ui-gateway-field-password")),
            password
        ]
        .spacing(spacing::XS),
    );
    match forms.passwords {
        PasswordStore::Ready if draft.password_saved => {
            form = form.push(
                row![
                    text(fl!("ui-profile-password-saved")).size(font_size::CAPTION),
                    button(text(fl!("ui-profile-password-clear")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press(Message::App(AppMessage::ClearGatewayPassword)),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            );
        }
        PasswordStore::Ready => {}
        PasswordStore::VaultLocked => {
            form = form.push(text(fl!("ui-profile-password-locked")).size(font_size::CAPTION));
        }
        PasswordStore::None => {
            form = form.push(text(fl!("ui-profile-password-no-store")).size(font_size::CAPTION));
        }
    }
    form = form.push(dialog_parts::hint(fl!("ui-gateway-password-hint")));
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
        .style(styles::text_input)
        .id(iced::widget::Id::new(passphrase.id))
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        input = input
            .on_input(passphrase.on_input)
            .on_submit(passphrase.submit.clone());
    }
    let mut field = column![
        dialog_parts::label(fl!("ui-profile-field-passphrase")),
        input
    ]
    .spacing(spacing::XS);
    match forms.passwords {
        PasswordStore::Ready if passphrase.saved => {
            field = field.push(
                row![
                    text(fl!("ui-profile-passphrase-saved")).size(font_size::CAPTION),
                    tooltip(
                        button(text(fl!("ui-profile-password-clear")).size(font_size::CAPTION))
                            .style(styles::subtle)
                            .on_press(Message::App(passphrase.clear.clone())),
                        text(fl!("ui-profile-passphrase-clear-tooltip")).size(font_size::CAPTION),
                        tooltip::Position::Top,
                    )
                    .style(container::rounded_box),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            );
        }
        PasswordStore::Ready => {}
        PasswordStore::VaultLocked => {
            field = field.push(text(fl!("ui-profile-password-locked")).size(font_size::CAPTION));
        }
        PasswordStore::None => {
            field = field.push(text(fl!("ui-profile-password-no-store")).size(font_size::CAPTION));
        }
    }
    field.push(dialog_parts::hint(fl!("ui-profile-passphrase-hint")))
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
            text(fl!("ui-gateway-field-parent")).size(font_size::CAPTION),
            pick_list(parents, selected, |choice| {
                Message::App(AppMessage::ChooseParentGateway(match choice {
                    ParentChoice::None => None,
                    ParentChoice::Gateway(gateway) => Some(gateway.id),
                }))
            })
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .width(Length::Fill),
        ]
        .spacing(spacing::XS),
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
    let mut form = Column::new().spacing(spacing::MD);
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
        DraftProtocol::Vnc => Some((
            fl!("ui-profile-credentials-vnc"),
            Some(fl!("ui-profile-credentials-vnc-desc")),
        )),
        DraftProtocol::Ftp => Some((
            fl!("ui-profile-credentials-ftp"),
            Some(fl!("ui-profile-credentials-ftp-desc")),
        )),
        DraftProtocol::Telnet | DraftProtocol::Local | DraftProtocol::Citrix => None,
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
                dialog_parts::label(fl!("ui-profile-winrm-identity")),
                pick_list(
                    [WinRmIdentity::Current, WinRmIdentity::Stored],
                    Some(selected),
                    |identity| Message::App(AppMessage::ProfileToggle {
                        toggle: ProfileToggle::StoredCredential,
                        on: identity == WinRmIdentity::Stored,
                    }),
                )
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .width(Length::Fill),
                // How the identity is proven, and what HTTP outside a domain needs, as the
                // C# dialog's hints.
                dialog_parts::hint(fl!("ui-profile-winrm-identity-hint")),
                dialog_parts::hint(fl!("ui-profile-winrm-trusted-hosts-hint")),
            ]
            .spacing(spacing::XS),
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
        form = form.push(dialog_parts::hint(fl!("ui-profile-domain-hint")));
    }
    if draft.shows_password() {
        form = form.push(password_field(draft, forms));
        if draft.protocol == DraftProtocol::WinRm {
            // Optional here, unlike the C# dialog: without one, PowerShell asks.
            form = form.push(dialog_parts::hint(fl!("ui-profile-winrm-password-hint")));
        }
    }

    form
}

/// The C# Options tab: session logging, then the protocol's options, as its C# card; an
/// RDP form's monitors among `monitors`.
fn options_section<'a>(
    draft: &'a ProfileDraft,
    monitors: &[crate::rdp_options::Monitor],
    resize_delay: u32,
) -> Column<'a, Message> {
    let mut tab = Column::new().spacing(SECTION_GAP);
    if draft.shows_session_logging() {
        tab = tab.push(session_logging_choice(draft));
    }
    tab = tab.push(protocol_options(draft, monitors, resize_delay));
    // A card of its own after the options, as the C# one.
    if draft.protocol == DraftProtocol::Ssh {
        tab = tab.push(crate::post_connect_form::view(&draft.post_connect));
    }
    tab
}

/// The protocol's options, as its C# card; an RDP form's monitors among `monitors`, its
/// wait after connecting left empty taking the settings' `resize_delay`.
fn protocol_options<'a>(
    draft: &'a ProfileDraft,
    monitors: &[crate::rdp_options::Monitor],
    resize_delay: u32,
) -> Column<'a, Message> {
    let mut form = Column::new().spacing(spacing::MD);
    let options = match draft.protocol {
        DraftProtocol::Rdp => Some((
            fl!("ui-profile-options-rdp"),
            fl!("ui-profile-options-rdp-desc"),
        )),
        DraftProtocol::Vnc => Some((
            fl!("ui-profile-options-vnc"),
            fl!("ui-profile-options-vnc-desc"),
        )),
        DraftProtocol::Telnet => Some((
            fl!("ui-profile-options-telnet"),
            fl!("ui-profile-options-telnet-desc"),
        )),
        DraftProtocol::Ftp => Some((
            fl!("ui-profile-options-ftp"),
            fl!("ui-profile-options-ftp-desc"),
        )),
        DraftProtocol::Ssh | DraftProtocol::Sftp => Some((
            fl!("ui-profile-options-ssh"),
            fl!("ui-profile-options-ssh-desc"),
        )),
        // Their own cards' titles.
        DraftProtocol::WinRm | DraftProtocol::Local | DraftProtocol::Citrix => None,
    };
    if let Some((options, description)) = options {
        form = form.push(section(options, Some(description)));
    }
    if draft.protocol == DraftProtocol::Rdp {
        // As the C# card: the choice first; the profile's own options stay shown, those the
        // defaults decide greyed and said not to be the ones in effect; then the C# tabs.
        form = form.push(toggle_box(
            draft,
            ProfileToggle::FollowDefaults,
            toggle_label(ProfileToggle::FollowDefaults),
        ));
        if draft.is_on(ProfileToggle::FollowDefaults) {
            form = form
                .push(dialog_parts::hint(fl!("ui-profile-rdp-defaults-banner")))
                .push(dialog_parts::hint(fl!(
                    "ui-profile-rdp-defaults-not-in-effect"
                )));
        }
        form = form
            .push(crate::rdp_options::display_audio(
                draft,
                monitors,
                move |field| {
                    if field == ProfileField::ResizeDelay {
                        // The C# watermark: the settings' value, which empty takes.
                        let global = fl!("ui-profile-resize-delay-global", ms = resize_delay);
                        labelled_field(draft, field, Some(global))
                    } else {
                        form_field(draft, field)
                    }
                },
            ))
            .push(crate::rdp_options::devices(draft))
            .push(crate::rdp_options::performance(draft))
            .push(crate::rdp_options::behavior(draft));
    }
    if draft.protocol == DraftProtocol::Citrix {
        form = form.push(crate::citrix_form::advanced(|field| {
            form_field(draft, field)
        }));
    }
    if draft.protocol == DraftProtocol::Ssh {
        form = form.push(ssh_mode_choice(draft));
    }
    for toggle in ProfileToggle::of(draft.protocol) {
        // The RDP groups draw their own boxes, in the C# tabs; the favourite mark is the
        // Info tab's, the WinRM transport the General tab's.
        let drawn_elsewhere = *toggle == ProfileToggle::StoredCredential
            || *toggle == ProfileToggle::Favorite
            || (draft.protocol == DraftProtocol::WinRm && WINRM_TRANSPORT.contains(toggle))
            || (draft.protocol == DraftProtocol::Rdp && crate::rdp_options::draws(*toggle));
        if !drawn_elsewhere && draft.shows_toggle(*toggle) {
            form = form.push(toggle_box(draft, *toggle, toggle_label(*toggle)));
            // What the box does, under it, as the C# dialog's hint.
            if let Some(hint) = toggle_hint(*toggle) {
                form = form.push(dialog_parts::hint(hint));
            }
        }
    }
    for warning in option_warnings(draft) {
        form = form.push(text(warning).size(font_size::CAPTION).style(text::danger));
    }
    if draft.protocol == DraftProtocol::Local {
        form = form.push(crate::local_form::advanced(draft, |field| {
            form_field(draft, field)
        }));
    }
    if draft.protocol == DraftProtocol::Telnet {
        form = form.push(
            text(fl!("ui-profile-telnet-warning"))
                .size(font_size::CAPTION)
                .style(text::danger),
        );
    }

    form
}

/// What the options chosen in `draft` weaken, each said under them: the legacy algorithms,
/// X11 forwarding, which lets the server's X clients reach this computer's display, and
/// NLA turned off.
fn option_warnings(draft: &ProfileDraft) -> Vec<String> {
    let mut warnings = Vec::new();
    if matches!(draft.protocol, DraftProtocol::Ssh | DraftProtocol::Sftp)
        && draft.is_on(ProfileToggle::LegacyAlgorithms)
    {
        warnings.push(fl!("ui-profile-legacy-algorithms-hint"));
    }
    if draft.warns_x11() {
        warnings.push(fl!("ui-profile-x11-warning"));
    }
    if draft.protocol == DraftProtocol::Rdp && !draft.is_on(ProfileToggle::Nla) {
        warnings.push(fl!("ui-profile-nla-off-hint"));
    }
    warnings
}

/// The profile form, as the C# server dialog: its title, the protocol, the tabs of
/// [`ProfileTab`] with the one shown, then where an imported profile came from, why the last
/// save was refused, Cancel and Save. Enter in a field saves.
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
    let tabs = ProfileTab::of(draft.protocol);
    // A tab the protocol has not is shown as General.
    let shown = if tabs.contains(&forms.profile_tab) {
        forms.profile_tab
    } else {
        ProfileTab::General
    };
    let page: Element<'a, Message> = match shown {
        ProfileTab::General => general_tab(draft, forms).into(),
        ProfileTab::Options => {
            options_section(draft, forms.monitors, forms.rdp_resize_delay).into()
        }
        ProfileTab::Network => network_section(draft, forms.gateways),
        ProfileTab::Info => info_tab(draft).into(),
    };
    // Once a save was refused, what is left to fix, as the C# counts it on each tab; a
    // refusal the form alone cannot see, a password with no account, until a field changes.
    let mut errors = if forms.profile_refused {
        draft.errors()
    } else {
        Vec::new()
    };
    if let Some(error) =
        error.filter(|error| !errors.iter().any(|known| known.field() == error.field()))
    {
        errors.push(error);
    }
    column![
        text(title).size(font_size::TITLE).font(styles::SEMIBOLD),
        protocol_line(draft.protocol, adding),
        tab_control(&tabs, shown, page, &errors),
        form_footer(draft, error),
    ]
    .spacing(spacing::MD)
    .height(Length::Fill)
    .into()
}

/// The protocol above the tabs, as the C# badge: its label, then the protocol's icon in the
/// accent and its name. In a new session it is a button back to the protocols.
fn protocol_line<'a>(protocol: DraftProtocol, adding: bool) -> Element<'a, Message> {
    let chip = row![
        icons::icon(
            Icon::of(protocol_kind(protocol)),
            Tint::Accent,
            icons::GLYPH_SIDE
        ),
        text(protocol_name(protocol)).font(styles::SEMIBOLD),
    ]
    .spacing(spacing::XS)
    .align_y(iced::Alignment::Center);
    let chip: Element<'a, Message> = if adding {
        tooltip(
            button(chip)
                .padding(CHIP_PADDING)
                .style(styles::subtle)
                .on_press(Message::App(AppMessage::NewProfile)),
            text(fl!("ui-profile-protocol-change-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    } else {
        container(chip).padding(CHIP_PADDING).into()
    };
    row![
        text(fl!("ui-profile-protocol-badge"))
            .size(font_size::CAPTION)
            .style(text::secondary),
        chip,
    ]
    .spacing(spacing::SM)
    .align_y(iced::Alignment::Center)
    .into()
}

/// The C# `ThemedTabControlStyle`: the tabs' headers on the window's background over a
/// line, then the tab shown on a card, scrolled when it is longer than the dialog.
fn tab_control<'a>(
    tabs: &[ProfileTab],
    shown: ProfileTab,
    page: Element<'a, Message>,
    errors: &[DraftError],
) -> Element<'a, Message> {
    let headers = tab_strip(
        tabs.iter().copied(),
        shown,
        ProfileTab::label,
        Message::ProfileTab,
        |tab| tab.error_count(errors),
    );
    container(column![
        container(headers)
            .padding(TAB_HEADERS_PADDING)
            .width(Length::Fill)
            .style(styles::tab_headers),
        container(iced::widget::space())
            .width(Length::Fill)
            .height(crate::tokens::BORDER_WIDTH)
            .style(styles::divider),
        styles::scroll(container(page).padding(spacing::LG).width(Length::Fill))
            .id(profile_page_id())
            .height(Length::Fill),
    ])
    .height(Length::Fill)
    .style(styles::tab_frame)
    .into()
}

/// Under the tabs, as the C# dialog's footer: where an imported profile came from, why the
/// last save was refused, then Cancel and Save at the bottom right.
fn form_footer<'a>(draft: &ProfileDraft, error: Option<DraftError>) -> Element<'a, Message> {
    let mut footer = Column::new().spacing(spacing::SM);
    if let Some(origin) = draft.metadata_kept.origin {
        footer = footer.push(
            text(texts::origin_name(origin))
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary)
                .font(iced::Font {
                    style: iced::font::Style::Italic,
                    ..crate::UI_FONT
                }),
        );
    }
    if let Some(error) = error {
        footer = footer.push(text(texts::draft_error(error)).style(text::danger));
    }
    // As in C#: Cancel, then Save; a profile is deleted from its menu.
    footer
        .push(dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::action(fl!("ui-profile-save-button"), styles::primary)
                .on_press(Message::SaveProfileForm),
        ]))
        .into()
}

/// The C# General tab: the connection basics, a Citrix application's or a local shell's
/// own card, then the protocol's credentials.
fn general_tab<'a>(draft: &'a ProfileDraft, forms: &Forms<'a>) -> Column<'a, Message> {
    let mut basics = column![
        section(
            fl!("ui-profile-section-basics"),
            Some(if draft.protocol.is_serverless() {
                fl!("ui-profile-section-basics-local-desc")
            } else {
                fl!("ui-profile-section-basics-desc")
            })
        ),
        form_field(draft, ProfileField::Name),
    ]
    .spacing(spacing::MD);
    // A local shell has no server.
    if draft.shows(ProfileField::Host) {
        basics = basics.push(
            row![
                container(form_field(draft, ProfileField::Host)).width(Length::Fill),
                container(form_field(draft, ProfileField::Port)).width(PORT_FIELD_WIDTH),
            ]
            .spacing(spacing::MD),
        );
        if draft.protocol == DraftProtocol::WinRm {
            basics = basics.push(winrm_transport(draft));
        }
        basics = basics.push(crate::address_test_view::view(draft, forms.gateways));
    }
    let mut tab = column![basics].spacing(SECTION_GAP);
    if draft.protocol == DraftProtocol::Citrix {
        tab = tab.push(crate::citrix_form::basics(|field| form_field(draft, field)));
    }
    if draft.protocol == DraftProtocol::Local {
        tab = tab.push(crate::local_form::basics(draft, |field| {
            form_field(draft, field)
        }));
    }
    tab.push(credentials_section(draft, forms))
}

/// A `WinRM` profile's transport under its port, as the C# General tab: HTTPS and the
/// certificate check, each with its hint, and TLS asked of the plaintext port said.
fn winrm_transport<'a>(draft: &ProfileDraft) -> Column<'a, Message> {
    let mut transport = Column::new().spacing(spacing::SM);
    for toggle in WINRM_TRANSPORT {
        if draft.shows_toggle(toggle) {
            transport = transport.push(toggle_box(draft, toggle, toggle_label(toggle)));
            if let Some(hint) = toggle_hint(toggle) {
                transport = transport.push(dialog_parts::hint(hint));
            }
        }
    }
    // TLS to the plaintext port: said, not corrected, as the C# schema check reports it.
    if draft.uses_ssl()
        && draft.port.trim() == heimdall_core::profile::DEFAULT_WINRM_HTTP_PORT.to_string()
    {
        transport = transport.push(
            text(fl!(
                "ui-profile-winrm-tls-on-http-port",
                http = heimdall_core::profile::DEFAULT_WINRM_HTTP_PORT,
                https = heimdall_core::profile::DEFAULT_WINRM_HTTPS_PORT
            ))
            .size(font_size::CAPTION)
            .style(text::danger),
        );
    }
    transport
}

/// The C# Info tab: the organization, its folder, environment and favourite mark, then the
/// metadata, its tags, MAC address and password manager's entry.
fn info_tab(draft: &ProfileDraft) -> Column<'_, Message> {
    let mut organization = column![
        section(
            fl!("ui-profile-section-organization"),
            Some(fl!("ui-profile-section-organization-desc"))
        ),
        form_field(draft, ProfileField::Group),
        // As the C#: the separator is taught by the example and by a sentence that stays.
        dialog_parts::hint(fl!("ui-profile-folder-hint")),
        environment_choice(draft),
    ]
    .spacing(spacing::MD);
    if draft.shows_toggle(ProfileToggle::Favorite) {
        organization = organization.push(toggle_box(
            draft,
            ProfileToggle::Favorite,
            toggle_label(ProfileToggle::Favorite),
        ));
    }
    let mut metadata = column![
        section(
            fl!("ui-profile-section-metadata"),
            Some(fl!("ui-profile-section-metadata-desc"))
        ),
        form_field(draft, ProfileField::Tags),
        form_field(draft, ProfileField::MacAddress),
    ]
    .spacing(spacing::MD);
    // With the C# metadata: the password manager's entry, for the protocols it serves.
    if draft.shows(ProfileField::VaultEntry) {
        metadata = metadata
            .push(form_field(draft, ProfileField::VaultEntry))
            .push(dialog_parts::hint(fl!("ui-profile-vault-entry-help")));
    }
    column![organization, metadata].spacing(SECTION_GAP)
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
            Some(dialog_parts::prompt(fl!("ui-dialog-permissions-label"))),
            fl!("ui-dialog-permissions-placeholder"),
        )
    } else {
        (None, fl!("ui-dialog-name-placeholder"))
    };
    dialog_parts::form(
        dialog_parts::title(title),
        column![]
            .push(label)
            .push(
                text_input(&placeholder, value)
                    .style(styles::text_input)
                    .id(name_field_id())
                    .on_input(|value| {
                        Message::App(AppMessage::Files(FilesMessage::NameEdited(value)))
                    })
                    .on_submit(Message::App(AppMessage::ConfirmDialog)),
            )
            .spacing(spacing::MD),
        confirm_buttons(confirm),
    )
}

/// Cancel, then `confirm` in the accent sending `ConfirmDialog`, at a dialog's bottom right.
fn confirm_buttons<'a>(confirm: String) -> iced::widget::Row<'a, Message> {
    dialog_parts::buttons([
        dialog_parts::cancel(),
        dialog_parts::confirm(confirm, Some(Message::App(AppMessage::ConfirmDialog))),
    ])
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
        dialog_parts::section(header, None),
        dialog_parts::dialog_label(label),
        text_input(&placeholder, value)
            .style(styles::text_input)
            .id(name_field_id())
            .on_input(|value| {
                Message::App(AppMessage::Selection(SelectionMessage::BulkEdited(value)))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
    ]
    .spacing(spacing::SM);
    if let Some(refused) = refused {
        body = body.push(dialog_parts::error(match refused {
            BulkRefusal::Port => fl!("ui-bulk-port-invalid"),
            BulkRefusal::Username => fl!("ui-bulk-username-invalid"),
        }));
    }
    body.push(confirm_buttons(fl!("ui-dialog-ok-button")))
        .into()
}

/// One password for `count` profiles, as the C# bulk password dialog asks it: for how
/// many, the profiles `skipped` and why, the password and again, and why it was refused.
/// OK waits for a password, as the C# Apply does.
fn bulk_password_dialog(
    count: usize,
    skipped: heimdall_app::BulkPasswordSkips,
    refused: Option<heimdall_app::BulkPasswordRefusal>,
    fields: &[Zeroizing<String>; 2],
) -> Element<'_, Message> {
    use heimdall_app::BulkPasswordRefusal;

    let mut body = column![dialog_parts::section(
        fl!("ui-bulk-password-header", count = count),
        None
    )]
    .spacing(spacing::SM);
    let lines = [
        (
            skipped.winrm,
            fl!("ui-bulk-password-skipped-winrm", count = skipped.winrm),
        ),
        (
            skipped.no_account,
            fl!(
                "ui-bulk-password-skipped-no-account",
                count = skipped.no_account
            ),
        ),
        (
            skipped.other,
            fl!("ui-bulk-password-skipped-other", count = skipped.other),
        ),
    ];
    for (skipped, line) in lines {
        if skipped > 0 {
            body = body.push(dialog_parts::hint(line));
        }
    }
    let labels = [
        fl!("ui-bulk-password-label"),
        fl!("ui-bulk-password-confirm-label"),
    ];
    let last = labels.len() - 1;
    for (index, label) in labels.into_iter().enumerate() {
        let input = text_input("", fields[index].as_str())
            .style(styles::text_input)
            .id(bulk_password_field_id(index))
            .secure(true)
            .on_input(move |value| Message::BulkPasswordField { index, value })
            .on_submit(if index < last {
                Message::FocusBulkPasswordField(index + 1)
            } else {
                Message::SubmitBulkPassword
            });
        body = body.push(column![dialog_parts::dialog_label(label), input].spacing(spacing::XS));
    }
    if let Some(refused) = refused {
        body = body.push(dialog_parts::error(match refused {
            BulkPasswordRefusal::Control => fl!("ui-bulk-password-control"),
            BulkPasswordRefusal::Mismatch => fl!("ui-bulk-password-mismatch"),
        }));
    }
    body.push(dialog_parts::buttons([
        dialog_parts::cancel(),
        dialog_parts::confirm(
            fl!("ui-dialog-ok-button"),
            (!fields[0].is_empty()).then_some(Message::SubmitBulkPassword),
        ),
    ]))
    .into()
}

fn bulk_password_field_id(index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("bulk-password-field-{index}"))
}

fn rename_profile_dialog(value: &str) -> Element<'_, Message> {
    dialog_parts::form(
        dialog_parts::title(fl!("ui-tree-rename-title")),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .style(styles::text_input)
            .id(name_field_id())
            .on_input(|value| {
                Message::App(AppMessage::ProfileMenu(ProfileMenuMessage::NameEdited(
                    value,
                )))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
        confirm_buttons(fl!("ui-dialog-rename-confirm")),
    )
}

/// The dialogs about the tree: a folder's name, its deletion, connecting all it holds, a
/// profile's name.
fn folder_dialog(dialog: &Dialog) -> Element<'_, Message> {
    let (severity, title, body, action) = match dialog {
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
                dialog_parts::prompt(fl!("ui-folder-name-field")),
                text_input(&fl!("ui-dialog-name-placeholder"), value)
                    .style(styles::text_input)
                    .id(name_field_id())
                    .on_input(
                        |value| Message::App(AppMessage::Folder(FolderMessage::NameEdited(value)))
                    )
                    .on_submit(Message::App(AppMessage::ConfirmDialog)),
            ]
            .spacing(spacing::SM);
            if let Some(error) = error {
                content = content.push(dialog_parts::error(match error {
                    FolderError::Collision => fl!("ui-folder-error-collision"),
                    _ => fl!("ui-folder-error-invalid"),
                }));
            }
            return dialog_parts::form(
                dialog_parts::title(title),
                content,
                confirm_buttons(action),
            );
        }
        Dialog::ConfirmDeleteFolder { name, count, .. } => (
            Severity::Danger,
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
            Severity::Danger,
            fl!("ui-dialog-delete-selection-title"),
            std::iter::once(fl!("ui-dialog-delete-selection-body", count = ids.len()))
                .chain(names.iter().map(|name| format!("- {name}")))
                .collect::<Vec<_>>()
                .join("\n"),
            fl!("ui-dialog-delete-profile-confirm"),
        ),
        Dialog::ConfirmConnectFolder { count, .. } => (
            Severity::Info,
            fl!("ui-folder-connect-all-title"),
            fl!("ui-folder-connect-all-body", count = (*count)),
            fl!("ui-folder-connect-all-confirm"),
        ),
        _ => return column![].into(),
    };
    dialog_parts::question(severity, title, body, action)
}

/// The dialogs about a tab: closing it or others, naming it, pasting several lines in it.
fn tab_dialog(dialog: &Dialog) -> Element<'_, Message> {
    let (severity, title, body, action) = match dialog {
        Dialog::RenameTab { value, .. } => return rename_tab_dialog(value),
        Dialog::SaveMacro { name, entries } => return save_macro_dialog(name, entries.len()),
        Dialog::CustomResolution { value, .. } => return custom_resolution_dialog(value),
        Dialog::ConfirmPaste {
            lines,
            command,
            preview,
            ..
        } => return paste_dialog(*lines, *command, preview),
        Dialog::ConfirmDisconnectDesktop { name, .. } => (
            Severity::Warning,
            fl!("ui-desktop-disconnect-title"),
            fl!("ui-desktop-disconnect-body", name = name.as_str()),
            fl!("ui-desktop-disconnect"),
        ),
        Dialog::ConfirmCloseTransfers { name, .. } => (
            Severity::Warning,
            fl!("ui-dialog-close-transfers-title"),
            fl!("ui-dialog-close-transfers-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmCloseEdits { name, .. } => (
            Severity::Warning,
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-edits-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmCloseTabs {
            tabs,
            live,
            unsaved,
        } => (
            Severity::Warning,
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
            Severity::Warning,
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-editor-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmDiscardEditor { .. } => (
            Severity::Danger,
            fl!("ui-dialog-discard-editor-title"),
            fl!("ui-dialog-discard-editor-body"),
            fl!("ui-editor-close"),
        ),
        Dialog::ConfirmOpenLink { url } => (
            Severity::Info,
            fl!("ui-dialog-open-link-title"),
            fl!("ui-dialog-open-link-body", url = server_text(url)),
            fl!("ui-dialog-open-link-confirm"),
        ),
        Dialog::ConfirmOpenRunnable { shown, .. } => (
            Severity::Warning,
            fl!("ui-dialog-open-runnable-title"),
            fl!("ui-dialog-open-runnable-body", path = shown.as_str()),
            fl!("ui-dialog-open-runnable-confirm"),
        ),
        Dialog::ConfirmDownloadBinary { name, .. } => (
            Severity::Info,
            fl!("ui-dialog-binary-title"),
            fl!("ui-dialog-binary-body", name = name.as_str()),
            fl!("ui-dialog-binary-confirm"),
        ),
        _ => (
            Severity::Warning,
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-tab-body"),
            fl!("ui-dialog-close-tab-confirm"),
        ),
    };
    dialog_parts::question(severity, title, body, action)
}

/// Pasting several lines into a shell that would run them, or a command that can destroy
/// data or stop the machine, as the C# `PasteConfirmDialog` asks it: what is at stake, the
/// destructive command named in a title of its own colour, the text itself in a box that
/// scrolls both ways, its lines never wrapped, and a word when some of it is out of sight.
fn paste_dialog<'a>(
    lines: usize,
    command: Option<&'static str>,
    preview: &'a PastePreview,
) -> Element<'a, Message> {
    // As the C# dialog: the warning's icon, or the error's with the title in its colour and
    // Paste red when the command can destroy.
    let (severity, title, body, action, style) = match command {
        Some(command) => (
            Severity::Error,
            dialog_parts::title(fl!("ui-dialog-paste-dangerous-title")).style(text::danger),
            fl!(
                "ui-dialog-paste-dangerous-body",
                command = command.to_string()
            ),
            fl!("ui-dialog-paste-dangerous-confirm"),
            styles::danger as fn(&Theme, button::Status) -> button::Style,
        ),
        None => (
            Severity::Warning,
            dialog_parts::title(fl!("ui-dialog-paste-title")),
            fl!("ui-dialog-paste-body", count = lines),
            fl!("ui-dialog-paste-confirm"),
            styles::primary as fn(&Theme, button::Status) -> button::Style,
        ),
    };
    let shown = styles::scroll(
        text(preview.lines.join("\n"))
            .font(iced::Font::MONOSPACE)
            .wrapping(text::Wrapping::None),
    )
    .direction(scrollable::Direction::Both {
        vertical: styles::scrollbar(),
        horizontal: styles::scrollbar(),
    })
    .width(Length::Fill)
    .height(Length::Shrink);
    let mut content = column![
        text(body),
        container(shown)
            .max_height(PASTE_PREVIEW_HEIGHT)
            .padding(spacing::MD)
            .style(container::rounded_box),
    ]
    .spacing(spacing::SM);
    if preview.truncated {
        content = content.push(dialog_parts::hint(fl!("ui-dialog-paste-truncated")).font(
            iced::Font {
                style: iced::font::Style::Italic,
                ..crate::UI_FONT
            },
        ));
    }
    dialog_parts::form(
        dialog_parts::header(severity, title),
        content,
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::action(action, style).on_press(Message::App(AppMessage::ConfirmDialog)),
        ]),
    )
}

/// "Reconnecting (attempt 2/20)...", as the C# countdown says it.
fn reconnecting(retry: Retry) -> String {
    fl!(
        "ui-session-reconnecting",
        attempt = retry.attempt,
        max = retry.max
    )
}

/// The seconds before `retry` starts, rounded up: "in 0s" would show while the wait still
/// runs.
fn seconds_left(retry: Retry) -> u64 {
    let left = retry
        .due
        .saturating_duration_since(std::time::Instant::now());
    left.as_secs() + u64::from(left.subsec_nanos() > 0)
}

/// The button that stops the attempts of `tab`.
fn cancel_retry_button<'a>(tab: TabId) -> iced::widget::Button<'a, Message> {
    button(text(fl!("ui-session-reconnecting-cancel")))
        .style(styles::secondary)
        .on_press(Message::App(AppMessage::CancelAutoReconnect(tab)))
}

/// A session waiting to open again by itself: which attempt, in how long, and Cancel.
fn countdown_card<'a>(tab: TabId, retry: Retry) -> Element<'a, Message> {
    center(card(
        column![
            text(reconnecting(retry)).size(font_size::TITLE),
            text(fl!(
                "ui-session-reconnecting-in",
                seconds = seconds_left(retry)
            )),
            cancel_retry_button(tab),
        ]
        .spacing(spacing::SM),
    ))
    .into()
}

/// The countdown of [`countdown_card`] in a bar, under a dropped session kept in sight.
fn countdown_bar<'a>(tab: TabId, retry: Retry) -> Element<'a, Message> {
    row![
        text(reconnecting(retry)),
        text(fl!(
            "ui-session-reconnecting-in",
            seconds = seconds_left(retry)
        )),
        cancel_retry_button(tab),
    ]
    .spacing(spacing::SM)
    .padding(spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

/// sudo's question, as the C# `PasswordInputDialog`: its password, typed hidden, kept for the
/// tab once sudo takes it.
fn sudo_password_dialog<'a>(name: &str, typed: &'a str) -> Element<'a, Message> {
    dialog_parts::form(
        dialog_parts::title(fl!("ui-dialog-sudo-title")),
        column![
            dialog_parts::prompt(fl!("ui-dialog-sudo-body", name = name)),
            text_input("", typed)
                .style(styles::text_input)
                .id(name_field_id())
                .secure(true)
                .on_input(Message::SudoPasswordEdited)
                .on_submit(Message::SudoPasswordConfirm),
        ]
        .spacing(spacing::MD),
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::confirm(
                fl!("ui-dialog-ok-button"),
                Some(Message::SudoPasswordConfirm),
            ),
        ]),
    )
}

/// One line typed under its `prompt` and `title`, as the C# `InputDialog`: `field`, then
/// Cancel and `confirm`.
fn input_dialog<'a>(
    title: String,
    prompt: impl Into<Element<'a, Message>>,
    field: iced::widget::TextInput<'a, Message>,
    confirm: String,
) -> Element<'a, Message> {
    dialog_parts::form(
        dialog_parts::title(title),
        column![
            prompt.into(),
            field
                .style(styles::text_input)
                .id(name_field_id())
                .on_submit(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(spacing::MD),
        confirm_buttons(confirm),
    )
}

/// The C# "Custom resolution" of an RDP tab: the size typed as `WIDTHxHEIGHT`.
fn custom_resolution_dialog(value: &str) -> Element<'_, Message> {
    input_dialog(
        fl!("ui-resolution-custom-title"),
        dialog_parts::prompt(fl!("ui-resolution-custom-prompt")),
        text_input("", value).on_input(|value| {
            Message::App(AppMessage::TabMenu(TabMenuMessage::ResolutionEdited(value)))
        }),
        fl!("ui-dialog-ok-button"),
    )
}

/// The name of the macro just recorded, of `count` inputs, asked before it is kept.
fn save_macro_dialog(value: &str, count: usize) -> Element<'_, Message> {
    input_dialog(
        fl!("ui-dialog-save-macro-title"),
        column![
            dialog_parts::prompt(fl!("ui-dialog-save-macro-prompt", count = count)),
            dialog_parts::hint(fl!("ui-dialog-save-macro-warning")),
        ]
        .spacing(spacing::XS),
        text_input(&fl!("ui-dialog-name-placeholder"), value).on_input(|value| {
            Message::App(AppMessage::Macro(heimdall_app::MacroMessage::NameEdited(
                value,
            )))
        }),
        fl!("ui-dialog-save-macro-confirm"),
    )
}

/// A name for a tab, as the C# "Rename Tab" asks it: the present one written in, an empty
/// one giving the tab its own title back.
fn rename_tab_dialog(value: &str) -> Element<'_, Message> {
    input_dialog(
        fl!("ui-dialog-rename-tab-title"),
        dialog_parts::prompt(fl!("ui-dialog-rename-tab-prompt")),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .on_input(|value| Message::App(AppMessage::TabMenu(TabMenuMessage::NameEdited(value)))),
        fl!("ui-dialog-rename-confirm"),
    )
}

/// The command a local profile would run, shown whole before it does: nothing cut, every
/// invisible character written out, and a warning when the program reads its line again.
fn local_command_dialog(confirmation: &LocalConfirmation) -> Element<'_, Message> {
    let intro = fl!("ui-dialog-local-body", name = confirmation.name.as_str());
    command_question(
        fl!("ui-dialog-local-title"),
        if confirmation.elevated {
            format!("{intro}\n{}", fl!("ui-dialog-local-elevated"))
        } else {
            intro
        },
        &confirmation.command,
        confirmation.folder.as_deref(),
        confirmation.rereads,
    )
}

/// The command a script of the local file browser would run, its interpreter's, shown whole
/// as a local profile's is before it runs; asked each time.
fn run_script_dialog(confirmation: &ScriptConfirmation) -> Element<'_, Message> {
    command_question(
        fl!("ui-dialog-run-script-title"),
        fl!(
            "ui-dialog-run-script-body",
            name = confirmation.name.as_str()
        ),
        &confirmation.command,
        confirmation.folder.as_deref(),
        confirmation.rereads,
    )
}

/// A question about running `command` on this computer, under `title` and `intro`: the
/// command whole in its own box, the `folder` it starts in, and a warning when the program
/// `rereads` its line.
fn command_question<'a>(
    title: String,
    intro: String,
    command: &'a str,
    folder: Option<&str>,
    rereads: bool,
) -> Element<'a, Message> {
    let mut body = column![
        dialog_parts::body(intro),
        container(
            styles::scroll(
                text(command)
                    .font(iced::Font::MONOSPACE)
                    .wrapping(text::Wrapping::Glyph)
            )
            .height(Length::Shrink)
        )
        .max_height(LOCAL_COMMAND_HEIGHT)
        .padding(spacing::MD)
        .style(container::rounded_box),
    ]
    .spacing(spacing::SM);
    if let Some(folder) = folder {
        body = body.push(dialog_parts::body(fl!(
            "ui-dialog-local-folder",
            folder = folder
        )));
    }
    if rereads {
        body = body.push(dialog_parts::error(fl!("ui-dialog-local-rereads")));
    }
    dialog_parts::message(
        Severity::Warning,
        title,
        body,
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::action(fl!("ui-dialog-local-confirm"), styles::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]),
    )
}

/// The C# question about an imported profile's post-connect commands, with the commands
/// shown: Yes types them and remembers the choice, No opens the shell without them.
fn post_connect_dialog(confirmation: &PostConnectConfirmation) -> Element<'_, Message> {
    let count = confirmation.commands.len();
    let body = column![
        dialog_parts::body(fl!(
            "ui-dialog-post-connect-body",
            name = confirmation.name.as_str(),
            count = count
        )),
        container(
            styles::scroll(
                text(confirmation.commands.join("\n"))
                    .font(iced::Font::MONOSPACE)
                    .wrapping(text::Wrapping::Glyph)
            )
            .height(Length::Shrink)
        )
        .max_height(LOCAL_COMMAND_HEIGHT)
        .padding(spacing::MD)
        .style(container::rounded_box),
    ]
    .spacing(spacing::SM);
    // As the C# `ShowConfirm` of the warning's severity.
    dialog_parts::message(
        Severity::Warning,
        fl!("ui-dialog-post-connect-title"),
        body,
        dialog_parts::buttons([
            dialog_parts::action(fl!("ui-dialog-post-connect-skip"), styles::secondary)
                .on_press(Message::App(AppMessage::SkipPostConnect)),
            dialog_parts::action(fl!("ui-dialog-post-connect-run"), styles::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]),
    )
}

/// The count of a tab's running post-connect steps, as the C# tab shows it; its tooltip says
/// which step and what became of it, a click stops the rest.
fn post_connect_badge(tab: TabId, progress: &PostConnectProgress) -> Element<'_, Message> {
    let count = format!("{}/{}", progress.step, progress.total);
    let status = texts::step_status(progress.status);
    tooltip(
        button(text(count.clone()).size(font_size::CAPTION))
            .style(styles::subtle)
            .on_press(Message::App(AppMessage::StopPostConnect(tab))),
        text(fl!(
            "ui-post-connect-tooltip",
            progress = count,
            status = status,
            command = progress.command.as_str()
        ))
        .size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The work of sudo in a Files tab: a file edited with it opened and saved, a folder
/// listed as root.
fn sudo_task(effect: Effect) -> Task<Message> {
    match effect {
        Effect::SudoListRemote {
            tab,
            shell,
            path,
            password,
        } => Task::perform(
            {
                let path = path.clone();
                async move {
                    heimdall_app::sudo_mode::sudo_list(
                        &shell,
                        &path,
                        password
                            .as_ref()
                            .map(heimdall_app::sudo_edit::SudoPassword::bytes),
                        heimdall_files::privileged::Sudo::System,
                    )
                    .await
                }
            },
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::SudoListed {
                    tab,
                    path: path.clone(),
                    result,
                }))
            },
        ),
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

/// Starts a program of this computer's for `tab` with `start`, away from the interface's
/// thread, then sends [`FilesMessage::EditorLaunched`]: a failure as `failed` says it.
fn started(
    tab: TabId,
    start: impl FnOnce() -> std::io::Result<()> + Send + 'static,
    failed: fn(String) -> FilesError,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(start)
                .await
                .map_err(std::io::Error::other)
                .and_then(|started| started)
                .map_err(|error| failed(error.to_string()))
        },
        move |result| {
            Message::App(AppMessage::Files(FilesMessage::EditorLaunched {
                tab,
                result,
            }))
        },
    )
}

/// The work of a file edited with the external editor: opening it, starting the editor
/// again, looking at its saves; and a local file or folder opened with a program.
fn edit_task(effect: Effect) -> Task<Message> {
    match effect {
        effect @ (Effect::SudoOpen { .. }
        | Effect::SudoSave { .. }
        | Effect::SudoListRemote { .. }) => sudo_task(effect),
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
        Effect::OpenFolder { tab, folder } => started(
            tab,
            move || heimdall_app::external_edit::open_folder(&folder),
            |detail| FilesError::OpenFailed { detail },
        ),
        Effect::OpenLocalFile { tab, file } => started(
            tab,
            move || heimdall_app::external_edit::open_with_default(&file),
            |detail| FilesError::OpenFailed { detail },
        ),
        Effect::OpenWithChooser { tab, file } => started(
            tab,
            move || heimdall_app::external_edit::open_with_chooser(&file),
            |detail| FilesError::OpenFailed { detail },
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
        Effect::LaunchEditor { tab, editor, file } => started(
            tab,
            move || heimdall_app::external_edit::launch(&editor, &file),
            |detail| FilesError::EditorFailed { detail },
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

/// The work of a Files tab: listing, transferring, changing entries; each change on the
/// server recorded in `journal` when given.
#[expect(clippy::too_many_lines, reason = "one arm per effect")]
fn files_task(effect: Effect, journal: Option<OperationJournal>) -> Task<Message> {
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
            let events =
                stream::once(async move { transfer_events_recorded(*request, journal) }).flatten();
            Task::stream(events).map(move |event| {
                Message::App(AppMessage::Files(FilesMessage::TransferEvent {
                    tab,
                    id,
                    event,
                }))
            })
        }
        Effect::FileBatchStep { tab, operation, .. } => Task::perform(
            file_operation_recorded(*operation, journal),
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::BatchStepDone {
                    tab,
                    result,
                }))
            },
        ),
        Effect::FileOperation {
            tab,
            side,
            operation,
        } => Task::perform(
            file_operation_recorded(*operation, journal),
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::OperationDone {
                    tab,
                    side,
                    result,
                }))
            },
        ),
        Effect::MoveRemote { tab, client, moves } => Task::perform(
            move_remote_recorded(client, moves, journal),
            move |results| Message::App(AppMessage::Files(FilesMessage::Moved { tab, results })),
        ),
        effect @ (Effect::StartEdit { .. }
        | Effect::LaunchEditor { .. }
        | Effect::CheckEdits { .. }
        | Effect::SudoOpen { .. }
        | Effect::SudoSave { .. }
        | Effect::SudoListRemote { .. }
        | Effect::SendEditAnyway { .. }
        | Effect::OpenFolder { .. }
        | Effect::OpenLocalFile { .. }
        | Effect::OpenWithChooser { .. }) => edit_task(effect),
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
            copy_remote_recorded(client, Some(shell), sources, folder, cancel, journal),
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

/// A step of Windows Hello for the vault, as the window hands it to the core.
fn vault_hello_message(message: VaultHelloMessage) -> Message {
    Message::App(AppMessage::VaultHello(message))
}

/// Opens the vault away from the window's thread: the key derivation takes a moment.
fn open_vault_task(
    path: PathBuf,
    password: Secret,
    job: VaultJob,
    ticket: VaultTicket,
) -> Task<Message> {
    Task::perform(open_vault(path, password, job), move |result| {
        Message::App(AppMessage::VaultOpened(ticket, result))
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
        | TabProfile::WinRm(_)
        | TabProfile::Citrix(_)
        | TabProfile::Tool(_) => false,
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

/// A quality in a VNC desktop's menu, by the C# toolbar's name for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QualityChoice(VncQuality);

impl fmt::Display for QualityChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            VncQuality::Best => fl!("ui-session-vnc-quality-best"),
            VncQuality::Balanced => fl!("ui-session-vnc-quality-balanced"),
            VncQuality::Performance => fl!("ui-session-vnc-quality-performance"),
            VncQuality::LowBandwidth => fl!("ui-session-vnc-quality-low-bandwidth"),
        })
    }
}

/// An environment in the form's list, as the C# names it; "(None)" for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnvironmentChoice(Option<heimdall_core::metadata::Environment>);

impl fmt::Display for EnvironmentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&texts::environment_name(self.0))
    }
}

/// The C# Organization section's environment: its label, then the list.
fn environment_choice(draft: &ProfileDraft) -> Element<'_, Message> {
    let choices: Vec<EnvironmentChoice> = std::iter::once(None)
        .chain(heimdall_core::metadata::Environment::ALL.map(Some))
        .map(EnvironmentChoice)
        .collect();
    column![
        dialog_parts::label(fl!("ui-profile-field-environment")),
        pick_list(
            choices,
            Some(EnvironmentChoice(draft.environment)),
            |EnvironmentChoice(environment)| Message::App(AppMessage::ProfileChoice(
                ProfileChoice::Environment(environment)
            )),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS)
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

/// Where an SSH profile's shell opens, as the C# "SSH mode" list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SshModeChoice(SshMode);

impl SshModeChoice {
    /// The choices, in the C# order.
    const ALL: [Self; 2] = [Self(SshMode::Embedded), Self(SshMode::External)];
}

impl fmt::Display for SshModeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            SshMode::Embedded => fl!("ui-profile-ssh-mode-embedded"),
            SshMode::External => fl!("ui-profile-ssh-mode-external"),
        })
    }
}

/// The C# "SSH mode" list of an SSH profile's form: in a tab, or in `PuTTY`, which is then
/// explained.
fn ssh_mode_choice(draft: &ProfileDraft) -> Element<'_, Message> {
    let mut mode = column![
        dialog_parts::label(fl!("ui-profile-ssh-mode")),
        pick_list(
            SshModeChoice::ALL.to_vec(),
            Some(SshModeChoice(draft.ssh_mode)),
            |SshModeChoice(mode)| Message::App(AppMessage::ProfileChoice(ProfileChoice::SshMode(
                mode
            ))),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS);
    if draft.ssh_mode == SshMode::External {
        mode = mode.push(dialog_parts::hint(fl!("ui-profile-ssh-mode-external-desc")));
    }
    mode.into()
}

/// Whether the profile's sessions keep a transcript, as the C# server dialog's choice.
fn session_logging_choice(draft: &ProfileDraft) -> Element<'_, Message> {
    column![
        dialog_parts::label(fl!("ui-profile-session-logging")),
        pick_list(
            LoggingChoice::ALL.to_vec(),
            Some(LoggingChoice(draft.session_logging)),
            |LoggingChoice(logging)| Message::App(AppMessage::ProfileChoice(
                ProfileChoice::SessionLogging(logging)
            )),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
        dialog_parts::hint(fl!("ui-profile-session-logging-hint")),
    ]
    .spacing(spacing::XS)
    .into()
}

/// An SSH agent preference in the Settings page's list, named as the C# Heimdall names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AgentChoice(AgentPreference);

/// The RDP connection timeouts offered, in seconds, 0 for none: within the C# range.
const CONNECT_TIMEOUTS: [u32; 9] = [0, 15, 30, 45, 60, 90, 120, 300, 600];

/// An RDP connection timeout as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeoutChoice(u32);

impl std::fmt::Display for TimeoutChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&if self.0 == 0 {
            fl!("ui-settings-rdp-connect-timeout-off")
        } else {
            fl!("ui-settings-rdp-connect-timeout-seconds", seconds = self.0)
        })
    }
}

/// A limit of sessions as the list names it: none at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionsChoice(u32);

impl std::fmt::Display for SessionsChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            0 => fl!("ui-settings-max-sessions-none"),
            max => max.to_string(),
        })
    }
}

/// A family of the terminals' text, as the list names it: by its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FontChoice(String);

impl std::fmt::Display for FontChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What Ctrl+V does, as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CtrlVChoice(CtrlVPaste);

impl std::fmt::Display for CtrlVChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            CtrlVPaste::Always => fl!("ui-settings-ctrl-v-always"),
            CtrlVPaste::OutsideFullScreenPrograms => fl!("ui-settings-ctrl-v-outside"),
            CtrlVPaste::Never => fl!("ui-settings-ctrl-v-never"),
        })
    }
}

/// What Ctrl+K does in a terminal, as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CtrlKChoice(CtrlKTerminal);

impl std::fmt::Display for CtrlKChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            CtrlKTerminal::QuickConnect => fl!("ui-settings-ctrl-k-quick-connect"),
            CtrlKTerminal::SendToSession => fl!("ui-settings-ctrl-k-send"),
        })
    }
}

/// An execution policy as the list names it, as the C# does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PolicyChoice(ExecutionPolicy);

impl std::fmt::Display for PolicyChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            ExecutionPolicy::Default => fl!("ui-settings-powershell-policy-default"),
            // `PowerShell`'s own words: not translated, as in the C#.
            other => other.name().to_owned(),
        })
    }
}

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

/// Widget identifier of the status bar's tunnels button, which shows a glyph and a number.
#[must_use]
pub fn tunnels_toggle_id() -> iced::widget::Id {
    iced::widget::Id::new("status-tunnels")
}

/// Widget identifier of the button hiding the sidebar, or showing it again: a glyph alone.
#[must_use]
pub fn sidebar_toggle_id() -> iced::widget::Id {
    iced::widget::Id::new("sidebar-toggle")
}

/// Widget identifier of the sidebar's Add button, which shows a glyph alone.
#[must_use]
pub fn tree_add_id() -> iced::widget::Id {
    iced::widget::Id::new("tree-add")
}

/// Widget identifier of the close button of tab `tab` on the strip, which shows a glyph
/// alone.
#[must_use]
pub fn tab_close_id(tab: TabId) -> iced::widget::Id {
    iced::widget::Id::from(format!("tab-close-{}", tab.value()))
}

/// Widget identifier of the main window's whole content: the fields Tab goes through under
/// a dialog, those of the tabs' own windows left out.
fn main_area_id() -> iced::widget::Id {
    iced::widget::Id::new("main-window")
}

/// The open dialog, whose fields alone Tab goes through.
fn dialog_area_id() -> iced::widget::Id {
    iced::widget::Id::new("dialog")
}

fn vault_field_id(index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("vault-field-{index}"))
}

/// What a vault dialog says: its title, what it explains, its fields' labels, its action
/// and what it says while the action runs.
struct VaultTexts {
    /// Its title.
    title: String,
    /// What it explains under its title, if anything.
    body: Option<String>,
    /// Its fields' labels, in order.
    labels: Vec<String>,
    /// Its main button's label.
    action: String,
    /// What it says while the action runs.
    busy: String,
}

/// What the vault dialog of `mode` says, as the C# dialog of each.
fn vault_texts(mode: VaultMode) -> VaultTexts {
    let master = || fl!("ui-vault-field-master");
    let new = || fl!("ui-vault-field-new");
    let confirm = || fl!("ui-vault-field-confirm");
    let (title, body, labels, action, busy) = match mode {
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
    VaultTexts {
        title,
        body,
        labels,
        action,
        busy,
    }
}

/// The vault's dialogs, as the C# Heimdall's: the master password asked at start, the lock
/// screen, and the master password enabled, changed or disabled.
fn vault_dialog<'a>(
    dialog: &'a VaultDialog,
    fields: &'a [Zeroizing<String>; 3],
) -> Element<'a, Message> {
    let VaultTexts {
        title,
        body,
        labels,
        action,
        busy,
    } = vault_texts(dialog.mode);
    // The new master password's field: its strength is said as it is typed, and the
    // confirmation follows it.
    let new_field = match dialog.mode {
        VaultMode::Create => Some(0),
        VaultMode::Change => Some(1),
        VaultMode::Unlock | VaultMode::Locked | VaultMode::Disable => None,
    };
    let count = labels.len();
    // As the C#: the unlock prompt centred, the rest at the left.
    let title = if matches!(dialog.mode, VaultMode::Unlock | VaultMode::Locked) {
        dialog_parts::centred_title(title)
    } else {
        dialog_parts::title(title)
    };
    let mut form = Column::new().spacing(spacing::SM);
    if let Some(body) = body {
        // Disabling says what it costs on a warning's card, the others explain in a caption.
        let said: Element<'a, Message> = if dialog.mode == VaultMode::Disable {
            dialog_parts::warning_card(body)
        } else {
            dialog_parts::hint(body).into()
        };
        form = form.push(said);
    }
    for (index, label) in labels.into_iter().enumerate() {
        let mut input = text_input("", fields[index].as_str())
            .style(styles::text_input)
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
        form = form.push(dialog_parts::field(label, input));
        if new_field == Some(index) {
            form = form.push(dialog_parts::hint(policy_line(fields[index].as_str())));
        }
    }
    if let Some(problem) = &dialog.problem {
        // A lockout on a warning's card, as the C# dialog; a refusal in the error colour.
        let said: Element<'a, Message> = match problem {
            VaultProblem::LockedOut { .. } => dialog_parts::warning_card(vault_problem(problem)),
            _ => dialog_parts::error(vault_problem(problem)).into(),
        };
        form = form.push(said);
    }
    if dialog.busy {
        let busy = if dialog.hello_waiting {
            fl!("ui-vault-hello-unlock-busy")
        } else {
            busy
        };
        form = form.push(dialog_parts::note(busy));
    }
    // As in C#: a new password is taken once it follows the rules and is typed twice alike.
    let ready = new_field.is_none_or(|index| {
        master_password_problem(fields[index].as_str()).is_none()
            && fields[index].as_str() == fields[index + 1].as_str()
    });
    dialog_parts::form(title, form, vault_buttons(dialog, action, ready))
}

/// The vault dialog's buttons: Windows Hello first when offered, at the left, as the C#
/// dialog offers it beside the master password; then, at the bottom right, Cancel, except
/// on the lock screen, and `action`, taken once `ready`. Locked out, no try is taken until
/// the minutes said are over, Windows Hello's included; nothing is while a try is under way.
fn vault_buttons(
    dialog: &VaultDialog,
    action: String,
    ready: bool,
) -> iced::widget::Row<'_, Message> {
    let locked_out = matches!(
        dialog.problem,
        Some(VaultProblem::LockedOut { until }) if until > std::time::SystemTime::now()
    );
    let mut buttons = Vec::new();
    // The lock screen has no Cancel; the one asked at start quits.
    if dialog.mode != VaultMode::Locked {
        buttons.push(dialog_parts::cancel());
    }
    buttons.push(dialog_parts::confirm(
        action,
        (!dialog.busy && ready && !locked_out).then_some(Message::SubmitVault),
    ));
    let buttons = dialog_parts::buttons(buttons);
    if !dialog.hello {
        return buttons;
    }
    let hello = dialog_parts::action(fl!("ui-vault-hello-unlock-button"), styles::secondary)
        .on_press_maybe((!dialog.busy && !locked_out).then_some(Message::App(
            AppMessage::VaultHello(VaultHelloMessage::Unlock),
        )));
    row![hello, buttons]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
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
    let mut form = Column::new().spacing(spacing::MD);
    for (index, label) in labels.into_iter().enumerate() {
        let input = text_input("", fields[index].as_str())
            .style(styles::text_input)
            .id(vault_field_id(index))
            .secure(true)
            .on_input(move |value| Message::VaultField { index, value })
            .on_submit(if index + 1 < count {
                Message::FocusVaultField(index + 1)
            } else {
                Message::SubmitPin
            });
        // As the C# PIN dialogs: the label in the secondary text above its box.
        form = form.push(column![dialog_parts::dialog_label(label), input].spacing(PIN_LABEL_GAP));
    }
    if let Some(problem) = &dialog.problem {
        // A lockout on a warning's card, as the C# dialog; a refusal in the error colour.
        let said: Element<'a, Message> = match problem {
            PinFailure::LockedOut { .. } => dialog_parts::warning_card(pin_problem_text(problem)),
            _ => dialog_parts::error(pin_problem_text(problem)).into(),
        };
        form = form.push(said);
    }
    // Locked out, no try is taken until the minutes said are over.
    let open = !matches!(
        dialog.problem,
        Some(PinFailure::LockedOut { until }) if until > std::time::SystemTime::now()
    );
    let mut buttons = Vec::new();
    if dialog.mode == (PinMode::Setup { current: true }) {
        buttons.push(
            dialog_parts::action(fl!("ui-pin-remove-button"), styles::secondary)
                .on_press_maybe(open.then_some(Message::RemovePin)),
        );
    }
    buttons.push(dialog_parts::cancel());
    buttons.push(dialog_parts::confirm(
        action,
        open.then_some(Message::SubmitPin),
    ));
    // As the C# PIN dialogs, the title centred.
    dialog_parts::form(
        dialog_parts::centred_title(title),
        form,
        dialog_parts::buttons(buttons),
    )
}

/// Space between a PIN's label and its box, as the C# label's bottom margin.
const PIN_LABEL_GAP: f32 = 6.0;

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
        VaultProblem::HelloLocked => fl!("ui-vault-problem-hello-locked"),
        VaultProblem::HelloNotFound => fl!("ui-vault-problem-hello-not-found"),
        VaultProblem::HelloFailed => fl!("ui-vault-problem-hello-failed"),
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

/// The title, text and action of a question the Settings page asks: resetting the RDP
/// defaults or every setting, writing the default SSH or RDP mode into every profile of its
/// protocol. `None` for any other dialog.
fn settings_question(dialog: &Dialog) -> Option<(String, String, String)> {
    let question = match dialog {
        Dialog::ConfirmResetRdpDefaults => (
            fl!("ui-dialog-reset-rdp-title"),
            fl!("ui-dialog-reset-rdp-body"),
            fl!("ui-settings-rdp-reset-defaults"),
        ),
        Dialog::ConfirmResetAllSettings => (
            fl!("ui-dialog-reset-all-title"),
            fl!("ui-dialog-reset-all-body"),
            fl!("ui-settings-reset-all"),
        ),
        Dialog::ConfirmApplySshMode {
            mode,
            changes,
            total,
        } => (
            fl!("ui-dialog-apply-ssh-mode-title"),
            fl!(
                "ui-dialog-apply-ssh-mode-body",
                mode = settings_page::ssh_mode_name(*mode),
                changes = (*changes),
                total = (*total)
            ),
            fl!("ui-settings-apply-mode-to-all"),
        ),
        Dialog::ConfirmApplyRdpMode {
            mode,
            changes,
            total,
        } => (
            fl!("ui-dialog-apply-rdp-mode-title"),
            fl!(
                "ui-dialog-apply-rdp-mode-body",
                mode = settings_page::rdp_mode_name(*mode),
                changes = (*changes),
                total = (*total)
            ),
            fl!("ui-settings-apply-mode-to-all"),
        ),
        _ => return None,
    };
    Some(question)
}

/// The title, text and action of a plain question: leaving the window with sessions live,
/// broadcasting input to every tab, recording every session, resetting the RDP settings,
/// writing the default SSH or RDP mode into every profile of its protocol, deleting
/// profiles or folders, terminating a Citrix session.
fn plain_question(dialog: &Dialog) -> (String, String, String) {
    if let Some(question) = settings_question(dialog) {
        return question;
    }
    match dialog {
        Dialog::ConfirmCitrixTerminate { force, .. } => {
            crate::citrix_view::terminate_question(*force)
        }
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
        Dialog::ConfirmVaultHelloEnrolAgain => (
            fl!("ui-vault-hello-enrol-again-title"),
            fl!("ui-vault-hello-enrol-again-body"),
            fl!("ui-vault-hello-enrol-again-button"),
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

/// A plain question drawn as the C# `MessageDialog`: the icon of its severity, its title,
/// its text, Cancel and its action.
fn plain_question_view<'a>(dialog: &Dialog) -> Element<'a, Message> {
    let (title, body, action) = plain_question(dialog);
    dialog_parts::question(plain_severity(dialog), title, body, action)
}

/// How serious a plain question is: a deletion destroys; leaving with sessions live,
/// broadcasting, resetting or rewriting settings warn; recording sessions informs.
fn plain_severity(dialog: &Dialog) -> Severity {
    match dialog {
        Dialog::ConfirmDeleteProfile { .. }
        | Dialog::ConfirmDelete { .. }
        | Dialog::ConfirmDeleteMacro(_)
        | Dialog::ConfirmDeleteGateway { .. } => Severity::Danger,
        Dialog::ConfirmSessionLogging => Severity::Info,
        _ => Severity::Warning,
    }
}

/// The OK that closes a dialog which only informs.
fn ok_button<'a>() -> iced::widget::Button<'a, Message> {
    dialog_parts::ok()
}

#[expect(clippy::too_many_lines, reason = "one arm per dialog")]
fn dialog_view<'a>(dialog: &'a Dialog, forms: &Forms<'a>) -> Element<'a, Message> {
    match dialog {
        Dialog::SudoPassword { name, .. } => sudo_password_dialog(name, forms.sudo_password),
        Dialog::ConfirmSudoDelete { names, more, .. } => {
            files_view::sudo_delete_question(names, *more)
        }
        Dialog::ConfirmCloseTab(_)
        | Dialog::ConfirmDisconnectDesktop { .. }
        | Dialog::ConfirmCloseTransfers { .. }
        | Dialog::ConfirmCloseEdits { .. }
        | Dialog::ConfirmCloseEditor { .. }
        | Dialog::ConfirmDiscardEditor { .. }
        | Dialog::ConfirmDownloadBinary { .. }
        | Dialog::ConfirmOpenLink { .. }
        | Dialog::ConfirmOpenRunnable { .. }
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
        | Dialog::ConfirmResetAllSettings
        | Dialog::ConfirmVaultHelloEnrolAgain
        | Dialog::ConfirmApplySshMode { .. }
        | Dialog::ConfirmApplyRdpMode { .. }
        | Dialog::ConfirmDeleteMacro(_)
        | Dialog::ConfirmDeleteGateway { .. }
        | Dialog::ConfirmDeleteProfile { .. }
        | Dialog::ConfirmCitrixTerminate { .. }
        | Dialog::ConfirmDelete { .. } => plain_question_view(dialog),
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
            algorithm,
        } => crate::tunnels_view::host_key(host, *port, fingerprint, algorithm),
        Dialog::AskName { action, value, .. } => name_dialog(*action, value),
        Dialog::EditProfile { draft, error } => profile_form(draft, *error, forms),
        Dialog::ConfirmLocalCommand(confirmation) => local_command_dialog(confirmation),
        Dialog::ConfirmRunScript(confirmation) => run_script_dialog(confirmation),
        Dialog::ConfirmPostConnect(confirmation) => post_connect_dialog(confirmation),
        Dialog::ForgetTrustedKey(key) => crate::trusted_keys_view::forget_question(key),
        Dialog::TrustedHostKeyDetails(entry) => crate::trusted_keys_view::details(entry),
        Dialog::ForgetTrustedServer { key, count } => {
            crate::trusted_keys_view::forget_server_question(key, *count)
        }
        Dialog::ImportDone(summary) => import_report(summary, ok_button()),
        Dialog::RestoreSessions(dialog) => crate::restore_view::view(dialog),
        Dialog::Shortcuts => crate::shortcuts_view::view(ok_button()),
        Dialog::FileProperties(properties) => {
            crate::files_view::properties(properties, ok_button())
        }
        Dialog::LocalFileProperties(properties) => {
            crate::files_view::local_properties(properties, ok_button())
        }
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
        | Dialog::CitrixImportNothing { .. }
        | Dialog::CitrixImportDone(_)
        | Dialog::PasswordSaveFailed { .. }
        | Dialog::StoreUnreadable { .. }
        | Dialog::StoreError { .. }
        | Dialog::StoreChanged { .. } => report(dialog, ok_button()),
        Dialog::SessionsPreview(_)
        | Dialog::RdpPreview(_)
        | Dialog::HostKeysPreview(_)
        | Dialog::ConfirmImportFile(_)
        | Dialog::ConfirmCitrixImport(_) => import_preview(dialog),
        Dialog::Vault(vault) => vault_dialog(vault, forms.vault),
        Dialog::BulkPassword {
            ids,
            skipped,
            refused,
        } => bulk_password_dialog(ids.len(), *skipped, *refused, forms.bulk_password),
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
    fn a_bitmap_file_gives_its_image_without_its_header_and_nothing_else_does() {
        let mut file = b"BM".to_vec();
        file.extend_from_slice(&[0; 12]);
        file.extend_from_slice(b"dib");
        assert_eq!(dib_of_bitmap_file(&file), Some(&b"dib"[..]));
        // A header with nothing after it, a short file, another format: no image.
        assert_eq!(dib_of_bitmap_file(&file[..BITMAP_FILE_HEADER_BYTES]), None);
        assert_eq!(dib_of_bitmap_file(b"BM"), None);
        assert_eq!(dib_of_bitmap_file(b""), None);
        let mut png = b"PNG image".to_vec();
        png.extend_from_slice(&[0; 20]);
        assert_eq!(dib_of_bitmap_file(&png), None);
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
    fn the_vnc_qualities_are_listed_by_the_csharp_toolbar_names_in_its_order() {
        assert_eq!(
            VncQuality::ALL.map(|quality| QualityChoice(quality).to_string()),
            ["Best Quality", "Balanced", "Performance", "Low Bandwidth"]
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
            require_tls: false,
            username: None,
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
    fn ctrl_e_n_and_k_reach_the_window_only_when_no_widget_took_them() {
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
        // Ctrl+K, and Ctrl+Shift+K, open Quick Connect: a window shortcut a terminal and a
        // desktop leave uncaptured, unless the settings keep Ctrl+K for the terminal.
        for (letter, modifiers) in [
            ("k", Modifiers::CTRL),
            ("K", Modifiers::CTRL | Modifiers::SHIFT),
        ] {
            assert!(
                matches!(
                    routed(letter, modifiers, event::Status::Ignored),
                    Some(Message::Shortcut(WindowShortcut::QuickConnect))
                ),
                "{modifiers:?}"
            );
            assert!(
                routed(letter, modifiers, event::Status::Captured).is_none(),
                "a shell's ^K, when the settings send it there, stays its own"
            );
        }
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

    #[test]
    fn an_event_of_another_window_than_the_main_one_is_dropped() {
        let (main, other) = (window::Id::unique(), window::Id::unique());
        assert!(matches!(
            main_only((Some(main), (main, Message::ToggleFullscreen))),
            Some(Message::ToggleFullscreen)
        ));
        assert!(
            main_only((Some(main), (other, Message::ToggleFullscreen))).is_none(),
            "another window's key is not the main window's"
        );
        assert!(
            main_only((Some(main), (other, Message::MainWindowClosed))).is_none(),
            "another window closing ends nothing"
        );
        assert!(
            main_only((Some(main), (other, Message::WindowOpened(other)))).is_none(),
            "another window's density is not the main window's"
        );
        // No main window named, as in tests: every event is taken, as before.
        assert!(matches!(
            main_only((None, (other, Message::ToggleFullscreen))),
            Some(Message::ToggleFullscreen)
        ));
    }
}
