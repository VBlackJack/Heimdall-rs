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

//! A tab's own window, as the C# `FloatingSessionWindow`: a header naming the session, its
//! state and its route, with Reattach at its right, above the session as its tab shows it.
//!
//! The window opens at the C# size, centred, and its place is not kept. Its close button
//! is the application's to answer: the tab goes back to the strip, then closes as any tab.
//! It has none of the main window's shortcuts, as the C# window has no input bindings;
//! behind the lock screen it shows a veil and takes nothing. A Files tab takes its own keys
//! there, as the C# file browser does, and the files dropped on the window; a terminal its
//! search bar, Ctrl+Shift+F, as the C# view hosted in the window does.
//!
//! What its session sends is let through by name, each message naming its tab: a message
//! added later, or one the main window would apply to its own tab shown, is dropped.

use std::path::PathBuf;

use heimdall_app::files::FilesKey;
use heimdall_app::{
    FilesMessage, FloatId, FloatMessage, Message as AppMessage, QuestionId, SessionState, Tab,
    TabId,
};
use iced::keyboard::key::Named;
use iced::widget::{button, center, column, container, opaque, row, text};
use iced::{Element, Length, Size, event, keyboard, mouse, window};

/// The widget identifiers of a Files tab's fields, lists and glyph buttons, made of its tab:
/// the pane drawn in a tab's own window never answers to an operation meant for the main
/// window's. And the widths of its columns, which its headers send resized.
pub use crate::files_view::{ColumnWidths, PaneField, PaneTool, field_id, list_id, tool_id};
/// The integrated editor's messages, which a tab's own window lets through for its tab.
pub use crate::integrated_editor::{EditorKey, EditorMessage};

use crate::finder::Finder;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::terminal_view::keys::WindowShortcut;
use crate::tokens::font_size;
use crate::tree_view::{CursorSpot, TreeMenu};

/// The size the window opens at, in logical pixels, as the C# one.
pub const WINDOW_SIZE: Size = Size::new(1024.0, 768.0);

/// The smallest the window is made, in logical pixels, as the C# one.
pub const MIN_WINDOW_SIZE: Size = Size::new(400.0, 300.0);

/// Height of the header, in logical pixels, as the C# one.
const HEADER_HEIGHT: f32 = 36.0;

/// Room at the header's ends, in logical pixels, as the C# one's margin.
const HEADER_PADDING: [f32; 2] = [0.0, 8.0];

/// Gap between the header's parts, in logical pixels.
const HEADER_SPACING: f32 = 6.0;

/// Room inside the Reattach button, as the C# one's padding.
const BUTTON_PADDING: [f32; 2] = [4.0, 12.0];

/// Between the session's name and its state, as the C# header writes it.
const TITLE_STATE_SEPARATOR: &str = " - ";

/// How a tab's own window opens: at the C# size, centred, its close button the
/// application's to answer.
#[must_use]
pub fn settings() -> window::Settings {
    window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(MIN_WINDOW_SIZE),
        position: window::Position::Centered,
        exit_on_close_request: false,
        icon: crate::window_icon(),
        ..window::Settings::default()
    }
}

/// What a tab's own window reports of itself; its session's widgets report the rest.
#[derive(Debug, Clone, PartialEq)]
pub enum FloatEvent {
    /// Its close button.
    CloseRequested,
    /// It gained the focus, or lost it.
    Focused(bool),
    /// Its screen draws this many physical pixels per logical one.
    Rescaled(f32),
    /// Shift, Ctrl, Alt or the logo key pressed or released over it.
    Modifiers(keyboard::Modifiers),
    /// A key of a Files tab's lists, as the C# `FileBrowserShortcutPolicy` takes it: one no
    /// widget took, Tab whatever took it, as the main window's.
    FilesKey(FilesKey),
    /// Ctrl+F no widget took: a Files tab's filter, as the C# file browser's.
    FindKey,
    /// Ctrl+Shift+F no widget took: the search bar over its terminal, opened or closed.
    TerminalFind,
    /// Ctrl+K or Ctrl+Shift+K no widget took: Quick Connect, in the main window.
    QuickConnect,
    /// Ctrl+Shift+A no widget took: what the status bar said lately, copied.
    CopyStatus,
    /// Escape, taken by a widget or not: a Files tab's menu, its path bar typed in, then
    /// its listing on its way, as the main window's.
    Escape,
    /// The left button pressed over it, whatever took it: where a drag of a Files tab's
    /// entry starts.
    PointerPressed,
    /// Files dragged from Explorer came over it, or left.
    FilesHovered(bool),
    /// A file dragged from Explorer dropped on it: one of a drop, gathered with the others.
    FileDropped(PathBuf),
    /// Characters typed that no widget took: a Files tab's type-ahead, as the main window's.
    TypeAhead(String),
}

/// The event of a tab's own window it reports; none of the main window's shortcuts but
/// Quick Connect.
pub(crate) fn window_event(
    event: iced::Event,
    status: event::Status,
    _window: window::Id,
) -> Option<FloatEvent> {
    match event {
        iced::Event::Window(window::Event::CloseRequested) => Some(FloatEvent::CloseRequested),
        iced::Event::Window(window::Event::Focused) => Some(FloatEvent::Focused(true)),
        iced::Event::Window(window::Event::Unfocused) => Some(FloatEvent::Focused(false)),
        iced::Event::Window(window::Event::Rescaled(scale)) => Some(FloatEvent::Rescaled(scale)),
        iced::Event::Window(window::Event::FileHovered(_)) => Some(FloatEvent::FilesHovered(true)),
        iced::Event::Window(window::Event::FilesHoveredLeft) => {
            Some(FloatEvent::FilesHovered(false))
        }
        iced::Event::Window(window::Event::FileDropped(path)) => {
            Some(FloatEvent::FileDropped(path))
        }
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            Some(FloatEvent::PointerPressed)
        }
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(FloatEvent::Modifiers(modifiers))
        }
        iced::Event::Keyboard(keyboard) => files_key_event(keyboard, status),
        _ => None,
    }
}

/// A key pressed over a tab's own window, as a Files tab takes it: what the main window
/// sends its Files tab shown, its shortcuts left out but the terminal's search bar.
fn files_key_event(event: keyboard::Event, status: event::Status) -> Option<FloatEvent> {
    let keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        repeat,
        text,
        ..
    } = event
    else {
        return None;
    };
    let untaken = status == event::Status::Ignored;
    let (ctrl, alt, logo) = (modifiers.control(), modifiers.alt(), modifiers.logo());
    match key {
        keyboard::Key::Named(Named::Escape) if !repeat => Some(FloatEvent::Escape),
        keyboard::Key::Named(Named::Enter) if untaken && !repeat => {
            Some(FloatEvent::FilesKey(FilesKey::Open))
        }
        // Whether or not a field took it, as in the main window: the other pane.
        keyboard::Key::Named(Named::Tab) if !(ctrl || alt || logo) => {
            Some(FloatEvent::FilesKey(FilesKey::SwitchPane))
        }
        _ if !untaken => None,
        _ if crate::terminal_view::keys::is_search_key(&key, physical_key, modifiers) => {
            Some(FloatEvent::FindKey)
        }
        _ if crate::terminal_view::keys::window_shortcut(&key, physical_key, modifiers)
            == Some(WindowShortcut::Find) =>
        {
            Some(FloatEvent::TerminalFind)
        }
        _ if crate::terminal_view::keys::window_shortcut(&key, physical_key, modifiers)
            == Some(WindowShortcut::QuickConnect) =>
        {
            (!repeat).then_some(FloatEvent::QuickConnect)
        }
        _ if crate::terminal_view::keys::window_shortcut(&key, physical_key, modifiers)
            == Some(WindowShortcut::CopyStatus) =>
        {
            (!repeat).then_some(FloatEvent::CopyStatus)
        }
        _ => crate::files_view::files_key(&key, physical_key, modifiers)
            .map(FloatEvent::FilesKey)
            .or_else(|| {
                crate::terminal_view::keys::typed_text(text.as_deref(), modifiers)
                    .map(|typed| FloatEvent::TypeAhead(typed.to_owned()))
            }),
    }
}

/// An event of a window other than `main`, as a tab's own window's; none of the main
/// window, nor any while no main window is named, as in tests, which open none.
pub(crate) fn from_floating(
    (main, (window, event)): (Option<window::Id>, (window::Id, FloatEvent)),
) -> Option<Message> {
    main.is_some_and(|main| main != window)
        .then_some(Message::Float(window, event))
}

/// Whether `message`, sent by the session drawn in the window of `tab`, may reach the
/// application: it names `tab` itself, or a question `tab` asks, or opens a dialog of the
/// main window that names nothing shown. Anything else is dropped, a message added later
/// included: it could act on the main window's tab shown.
#[must_use]
pub fn floating_message_allowed(message: &Message, tab: &Tab) -> bool {
    match message {
        // Its terminal, its desktop and its session bar, its cards: each names its tab.
        Message::App(
            AppMessage::Key { tab: named, .. }
            | AppMessage::Pointer { tab: named, .. }
            | AppMessage::Resize { tab: named, .. }
            | AppMessage::Copy(named)
            | AppMessage::ClipboardText { tab: named, .. }
            | AppMessage::ScrollHistory { tab: named, .. }
            | AppMessage::DesktopInput { tab: named, .. }
            | AppMessage::DesktopResize { tab: named, .. }
            | AppMessage::DesktopShown { tab: named, .. }
            | AppMessage::SendKeys { tab: named, .. }
            | AppMessage::VncQuality { tab: named, .. }
            | AppMessage::SendClipboard(named)
            | AppMessage::SaveRemoteFiles(named)
            | AppMessage::CancelSave(named)
            | AppMessage::StopAntiIdle(named)
            | AppMessage::DisconnectDesktop(named)
            | AppMessage::CitrixTerminate { tab: named, .. }
            | AppMessage::RequestCloseTab(named)
            | AppMessage::ReconnectTab(named)
            | AppMessage::ForgetServer(named)
            | AppMessage::CancelAutoReconnect(named)
            | AppMessage::CopyHostKeyFingerprint(named)
            | AppMessage::HostKeyDecision { tab: named, .. }
            | AppMessage::HostKeyTrustOnce(named),
        )
        | Message::Submit(named)
        | Message::Decline(named)
        | Message::DesktopFit { tab: named, .. }
        | Message::CopyError(named)
        | Message::CopyAnonymousError(named)
        // Its Files pane's path bar opened, its columns resized, its menus opened, and its
        // integrated editor's edits and keys.
        | Message::EditPath { tab: named, .. }
        | Message::FileColumns { tab: named, .. }
        | Message::OpenTreeMenu(
            TreeMenu::FilesEntry { tab: named, .. }
            | TreeMenu::FilesBookmarks(named)
            | TreeMenu::FilesBookmarksRemove(named),
        )
        | Message::Editor(
            EditorMessage::Action { tab: named, .. } | EditorMessage::Key { tab: named, .. },
        )
        // The search bar over its terminal, drawn in its window.
        | Message::FinderQuery { tab: named, .. }
        | Message::FinderFind { tab: named, .. }
        | Message::FinderClose(named) => *named == tab.id,
        // What is typed into its question, which only it asks.
        Message::Field { question, .. } | Message::FocusField { question, .. } => tab
            .prompts
            .iter()
            .any(|prompt| prompt.question == *question),
        // The main window's dialogs, naming a profile or nothing: the keyboard's help from
        // the session bar, the profile form from the failure card.
        Message::App(AppMessage::ShowShortcuts | AppMessage::EditProfile(_)) => true,
        // Its Files panes and their menus' entries, each naming its tab; the pointer over
        // them.
        Message::App(AppMessage::Files(files)) => pane_names(files, tab.id),
        Message::MenuChoice(AppMessage::Files(files)) => menu_names(files, tab.id),
        Message::FilesHover(spot) | Message::FilesHoverLeft(spot) => spot.tab == tab.id,
        _ => false,
    }
}

/// Whether `message`, sent by a Files pane drawn in a tab's own window, names `tab`: one of
/// those its lists, its buttons, its path bar, its filter, its transfers and its external
/// edits send. Any other is dropped, one added later included.
fn pane_names(message: &FilesMessage, tab: TabId) -> bool {
    match message {
        FilesMessage::Select { tab: named, .. }
        | FilesMessage::SortBy { tab: named, .. }
        | FilesMessage::Back { tab: named, .. }
        | FilesMessage::Up { tab: named, .. }
        | FilesMessage::Home { tab: named, .. }
        | FilesMessage::Ascend { tab: named, .. }
        | FilesMessage::PathEdited { tab: named, .. }
        | FilesMessage::GoTo { tab: named, .. }
        | FilesMessage::Refresh { tab: named, .. }
        | FilesMessage::Filter { tab: named, .. }
        | FilesMessage::ToggleHidden { tab: named, .. }
        | FilesMessage::AskNewFolder { tab: named, .. }
        | FilesMessage::AskRename { tab: named, .. }
        | FilesMessage::AskDelete { tab: named, .. }
        | FilesMessage::Bookmark { tab: named }
        | FilesMessage::ToggleFollow { tab: named }
        | FilesMessage::ToggleSudo { tab: named }
        | FilesMessage::ToggleLocal { tab: named }
        | FilesMessage::Transfer { tab: named, .. }
        | FilesMessage::Cancel { tab: named, .. }
        | FilesMessage::Retry { tab: named, .. }
        | FilesMessage::ClearFinished { tab: named }
        | FilesMessage::StopBatch { tab: named }
        | FilesMessage::EditSaveWithSudo { tab: named, .. }
        | FilesMessage::EditSendAnyway { tab: named, .. }
        | FilesMessage::EditOpenFolder { tab: named, .. }
        | FilesMessage::EditStop { tab: named, .. } => *named == tab,
        _ => false,
    }
}

/// Whether `message`, an entry of a Files pane's menu drawn in a tab's own window, names
/// `tab`: one of those its entries' menu, its folder's and its bookmarks' send.
fn menu_names(message: &FilesMessage, tab: TabId) -> bool {
    match message {
        FilesMessage::Open { tab: named, .. }
        | FilesMessage::EditIntegrated { tab: named }
        | FilesMessage::EditExternal { tab: named }
        | FilesMessage::EditWithSudo { tab: named }
        | FilesMessage::Transfer { tab: named, .. }
        | FilesMessage::AskRename { tab: named, .. }
        | FilesMessage::AskDelete { tab: named, .. }
        | FilesMessage::AskPermissions { tab: named, .. }
        | FilesMessage::UploadHere { tab: named }
        | FilesMessage::PasteFromExplorer { tab: named }
        | FilesMessage::Cut { tab: named }
        | FilesMessage::Copy { tab: named }
        | FilesMessage::Paste { tab: named }
        | FilesMessage::Duplicate { tab: named }
        | FilesMessage::CopyPath { tab: named, .. }
        | FilesMessage::ShowProperties { tab: named, .. }
        | FilesMessage::AskNewFolder { tab: named, .. }
        | FilesMessage::Refresh { tab: named, .. }
        | FilesMessage::OpenInTerminal { tab: named }
        | FilesMessage::OpenInExplorer { tab: named }
        | FilesMessage::OpenWith { tab: named, .. }
        | FilesMessage::OpenInEditor { tab: named, .. }
        | FilesMessage::OpenBookmark { tab: named, .. }
        | FilesMessage::RemoveBookmark { tab: named, .. } => *named == tab,
        _ => false,
    }
}

/// The tab of a Files pane `menu` is about: drawn in that tab's own window when it has one.
#[must_use]
pub fn files_menu_tab(menu: &TreeMenu) -> Option<TabId> {
    match *menu {
        TreeMenu::FilesEntry { tab, .. }
        | TreeMenu::FilesBookmarks(tab)
        | TreeMenu::FilesBookmarksRemove(tab) => Some(tab),
        _ => None,
    }
}

/// What the window keeps of a tab's own window.
#[derive(Debug, Clone)]
pub(crate) struct FloatingWindow {
    /// The application's name for it.
    pub key: FloatId,
    /// Physical pixels per logical one on its screen: its desktop is drawn one of its
    /// pixels per physical one.
    pub scale: f32,
    /// The question of its tab whose first field was last given the focus.
    pub question: Option<QuestionId>,
    /// Files dragged from Explorer are over it.
    pub hovered: bool,
    /// Where the pointer last was in it: where a menu opens, and a drag starts.
    pub cursor: CursorSpot,
    /// The search bar over its terminal, when open: its own, the main window's bar left
    /// as it is.
    pub finder: Option<Finder>,
}

impl FloatingWindow {
    /// The window the application calls `key`, on a screen of `scale`.
    pub fn new(key: FloatId, scale: f32) -> Self {
        Self {
            key,
            scale,
            question: None,
            hovered: false,
            cursor: CursorSpot::default(),
            finder: None,
        }
    }
}

/// What a tab's own window says of its session, in its header.
#[derive(Debug, Clone)]
pub struct Header {
    /// The window.
    pub key: FloatId,
    /// The session's state, as its dot shows it.
    pub state: SessionState,
    /// Its protocol, as its tab names it.
    pub kind: String,
    /// Its name.
    pub title: String,
    /// The gateways it goes through, named; `None` when it goes straight.
    pub route: Option<String>,
}

/// The state of a session, as the C# header words it.
#[must_use]
pub fn state_text(state: SessionState) -> String {
    match state {
        SessionState::Connected => fl!("ui-status-connected-short"),
        SessionState::Connecting => fl!("ui-status-connecting"),
        SessionState::Reconnecting => fl!("ui-status-reconnecting"),
        SessionState::Ended => fl!("ui-status-disconnected"),
        SessionState::Failed => fl!("ui-status-error"),
    }
}

/// The header of a tab's own window, as the C# one: the protocol and the name, the state,
/// the route, and Reattach at the right.
fn header<'a>(header: Header) -> Element<'a, Message> {
    let mut named = row![
        crate::tree_view::state_dot(Some(header.state)),
        text(header.kind).size(font_size::CAPTION),
        text(header.title).size(font_size::SUBTITLE),
        text(TITLE_STATE_SEPARATOR)
            .size(font_size::CAPTION)
            .style(text::secondary),
        text(state_text(header.state)).size(font_size::CAPTION),
    ]
    .spacing(HEADER_SPACING)
    .align_y(iced::Alignment::Center);
    if let Some(route) = header.route {
        named = named.push(
            text(route)
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary),
        );
    }
    let reattach = button(text(fl!("ui-detach-reattach")).size(font_size::CAPTION))
        .style(styles::secondary)
        .padding(BUTTON_PADDING)
        .on_press(Message::App(AppMessage::Float(FloatMessage::Reattach(
            header.key,
        ))));
    container(
        row![named, iced::widget::space::horizontal(), reattach].align_y(iced::Alignment::Center),
    )
    .padding(HEADER_PADDING)
    .height(HEADER_HEIGHT)
    .width(Length::Fill)
    .align_y(iced::alignment::Vertical::Center)
    .style(container::rounded_box)
    .into()
}

/// A tab's own window: its header above `body`, the session as its tab shows it.
#[must_use]
pub fn view(heading: Header, body: Element<'_, Message>) -> Element<'_, Message> {
    column![
        header(heading),
        container(body).width(Length::Fill).height(Length::Fill)
    ]
    .into()
}

/// A tab's own window behind the lock screen: a veil over all of it, taking every click,
/// and nothing of the session drawn, so no field of it takes what is typed.
#[must_use]
pub fn veil<'a>() -> Element<'a, Message> {
    opaque(
        container(center(
            text(fl!("ui-vault-locked-title")).size(font_size::TITLE),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(theme.palette().background.into()),
            ..container::Style::default()
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{Code, NativeCode, Physical};
    use iced::keyboard::{Key, Location, Modifiers};

    fn pressed(key: Key, physical: Physical, modifiers: Modifiers) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: physical,
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    fn named(key: Named, modifiers: Modifiers, status: event::Status) -> Option<FloatEvent> {
        let unknown = Physical::Unidentified(NativeCode::Unidentified);
        window_event(
            pressed(Key::Named(key), unknown, modifiers),
            status,
            window::Id::unique(),
        )
    }

    #[test]
    fn letters_no_field_took_are_its_files_tabs_type_ahead() {
        let (untaken, taken) = (event::Status::Ignored, event::Status::Captured);
        let letter = |modifiers| {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character("N".into()),
                modified_key: Key::Character("N".into()),
                physical_key: Physical::Code(Code::KeyN),
                location: Location::Standard,
                modifiers,
                text: Some("N".into()),
                repeat: false,
            })
        };
        assert_eq!(
            window_event(letter(Modifiers::SHIFT), untaken, window::Id::unique()),
            Some(FloatEvent::TypeAhead("N".to_owned()))
        );
        assert_eq!(
            window_event(letter(Modifiers::SHIFT), taken, window::Id::unique()),
            None,
            "a field's"
        );
        assert_eq!(
            window_event(letter(Modifiers::ALT), untaken, window::Id::unique()),
            None,
            "no shortcut's"
        );
    }

    #[test]
    fn a_files_tabs_keys_come_from_its_window_as_the_main_windows_do() {
        let (untaken, taken) = (event::Status::Ignored, event::Status::Captured);
        let none = Modifiers::empty();
        assert_eq!(
            named(Named::ArrowDown, none, untaken),
            Some(FloatEvent::FilesKey(FilesKey::Next))
        );
        assert_eq!(
            named(Named::F2, none, untaken),
            Some(FloatEvent::FilesKey(FilesKey::Rename))
        );
        assert_eq!(named(Named::ArrowDown, none, taken), None, "a field's");
        assert_eq!(
            named(Named::Enter, none, untaken),
            Some(FloatEvent::FilesKey(FilesKey::Open))
        );
        assert_eq!(named(Named::Enter, none, taken), None, "a field's submit");
        // Tab and Escape whatever took them, as in the main window.
        assert_eq!(
            named(Named::Tab, none, taken),
            Some(FloatEvent::FilesKey(FilesKey::SwitchPane))
        );
        assert_eq!(named(Named::Tab, Modifiers::CTRL, untaken), None);
        assert_eq!(named(Named::Escape, none, taken), Some(FloatEvent::Escape));
        // Ctrl+F: the filter's, whatever the layout.
        let find = pressed(
            Key::Character("f".into()),
            Physical::Code(Code::KeyF),
            Modifiers::CTRL,
        );
        assert_eq!(
            window_event(find.clone(), untaken, window::Id::unique()),
            Some(FloatEvent::FindKey)
        );
        assert_eq!(window_event(find, taken, window::Id::unique()), None);
        // None of the main window's shortcuts: F11, F1, Ctrl+L.
        assert_eq!(named(Named::F11, none, untaken), None);
        assert_eq!(named(Named::F1, none, untaken), None);
        let lock = pressed(
            Key::Character("l".into()),
            Physical::Code(Code::KeyL),
            Modifiers::CTRL,
        );
        assert_eq!(window_event(lock, untaken, window::Id::unique()), None);
    }

    #[test]
    fn ctrl_shift_a_left_by_its_session_copies_the_status_once() {
        let (untaken, taken) = (event::Status::Ignored, event::Status::Captured);
        let copy = pressed(
            Key::Character("A".into()),
            Physical::Code(Code::KeyA),
            Modifiers::CTRL | Modifiers::SHIFT,
        );
        assert_eq!(
            window_event(copy.clone(), untaken, window::Id::unique()),
            Some(FloatEvent::CopyStatus)
        );
        assert_eq!(window_event(copy, taken, window::Id::unique()), None);
        let held = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character("A".into()),
            modified_key: Key::Character("A".into()),
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            text: None,
            repeat: true,
        });
        assert_eq!(
            window_event(held, untaken, window::Id::unique()),
            None,
            "held, copied once"
        );
    }

    #[test]
    fn ctrl_k_left_by_its_session_is_quick_connect_of_the_main_window() {
        let (untaken, taken) = (event::Status::Ignored, event::Status::Captured);
        let k = Physical::Code(Code::KeyK);
        for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
            let quick = pressed(Key::Character("k".into()), k, modifiers);
            assert_eq!(
                window_event(quick.clone(), untaken, window::Id::unique()),
                Some(FloatEvent::QuickConnect),
                "{modifiers:?}"
            );
            assert_eq!(
                window_event(quick, taken, window::Id::unique()),
                None,
                "a shell's ^K, when the settings send it there"
            );
        }
        let held = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character("k".into()),
            modified_key: Key::Character("k".into()),
            physical_key: k,
            location: Location::Standard,
            modifiers: Modifiers::CTRL,
            text: None,
            repeat: true,
        });
        assert_eq!(
            window_event(held, untaken, window::Id::unique()),
            None,
            "once, held or not"
        );
    }

    #[test]
    fn ctrl_shift_f_left_by_its_widgets_is_its_terminals_search_bar() {
        let find = pressed(
            Key::Character("F".into()),
            Physical::Code(Code::KeyF),
            Modifiers::CTRL | Modifiers::SHIFT,
        );
        assert_eq!(
            window_event(find.clone(), event::Status::Ignored, window::Id::unique()),
            Some(FloatEvent::TerminalFind)
        );
        assert_eq!(
            window_event(find, event::Status::Captured, window::Id::unique()),
            None,
            "a field's"
        );
        // Whatever the layout: the key's place.
        let elsewhere = pressed(
            Key::Character("\u{430}".into()),
            Physical::Code(Code::KeyF),
            Modifiers::CTRL | Modifiers::SHIFT,
        );
        assert_eq!(
            window_event(elsewhere, event::Status::Ignored, window::Id::unique()),
            Some(FloatEvent::TerminalFind)
        );
    }

    #[test]
    fn its_drops_and_presses_come_from_its_window() {
        let path = std::path::PathBuf::from("report.pdf");
        let routed = |event| window_event(event, event::Status::Ignored, window::Id::unique());
        assert_eq!(
            routed(iced::Event::Window(window::Event::FileHovered(
                path.clone()
            ))),
            Some(FloatEvent::FilesHovered(true))
        );
        assert_eq!(
            routed(iced::Event::Window(window::Event::FilesHoveredLeft)),
            Some(FloatEvent::FilesHovered(false))
        );
        assert_eq!(
            routed(iced::Event::Window(window::Event::FileDropped(
                path.clone()
            ))),
            Some(FloatEvent::FileDropped(path))
        );
        assert_eq!(
            window_event(
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                event::Status::Captured,
                window::Id::unique()
            ),
            Some(FloatEvent::PointerPressed)
        );
    }
}
