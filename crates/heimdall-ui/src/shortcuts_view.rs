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

//! The keyboard shortcuts help, as the C# F1 dialog: every key the window answers to, by
//! where it acts. Only the keys bound here are listed, so the list never promises one that
//! does nothing.

use iced::widget::{Column, button, column, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of the keys' column.
const KEYS_WIDTH: f32 = 220.0;
/// Height of the list before it scrolls.
const LIST_HEIGHT: f32 = 460.0;

/// Room around the status bar's hint: the bar's own height, and a little to its sides.
const HINT_PADDING: [f32; 2] = [0.0, 4.0];

/// Where a group of keys acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The sessions list.
    Sessions,
    /// The tabs.
    Tabs,
    /// A terminal.
    Terminal,
    /// A Files tab.
    Files,
    /// The window.
    Window,
}

/// What a key does, for the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NewSession,
    EditSession,
    QuickConnect,
    Search,
    UndoMove,
    SelectAllSessions,
    SessionMenu,
    FindByName,
    ToggleSidebar,
    NextTab,
    PreviousTab,
    CloseTab,
    ToggleSplit,
    NextPane,
    PreviousPane,
    FindInTerminal,
    TextSize,
    Broadcast,
    TerminalCopy,
    TerminalPaste,
    ScrollHistory,
    FilesCopyCut,
    FilesPaste,
    FilesSelectAll,
    FilesCopyPath,
    FilesDownloadUpload,
    FilesRename,
    FilesNewFolder,
    FilesDelete,
    FilesRefresh,
    FilesBack,
    FilesPath,
    FilesSwitchPane,
    FullScreen,
    /// Ctrl+Alt+Home: the keyboard back from a remote desktop.
    ReleaseDesktop,
    Settings,
    Screenshot,
    /// Ctrl+Shift+A: what the status bar said lately, copied for a screen reader.
    CopyStatus,
    Lock,
    Help,
    Close,
}

/// The keys, as they are written on a keyboard, and what they do, by group, in the order
/// listed. A key's name is the same in every language the application speaks.
pub const SHORTCUTS: [(Group, &[(&str, Action)]); 5] = [
    (
        Group::Sessions,
        &[
            ("Ctrl+N", Action::NewSession),
            ("Ctrl+E", Action::EditSession),
            ("Ctrl+K, Ctrl+Shift+K", Action::QuickConnect),
            ("Ctrl+F", Action::Search),
            ("Ctrl+Z", Action::UndoMove),
            ("Ctrl+A", Action::SelectAllSessions),
            ("Shift+F10", Action::SessionMenu),
            ("A-Z, 0-9", Action::FindByName),
            ("Ctrl+B", Action::ToggleSidebar),
        ],
    ),
    (
        Group::Tabs,
        &[
            ("Ctrl+Tab, Ctrl+PgDn", Action::NextTab),
            ("Ctrl+Shift+Tab, Ctrl+PgUp", Action::PreviousTab),
            ("Ctrl+W, Ctrl+Shift+W", Action::CloseTab),
            ("Ctrl+Shift+O", Action::ToggleSplit),
            ("Ctrl+Alt+Right, Ctrl+F6", Action::NextPane),
            ("Ctrl+Alt+Left, Ctrl+Shift+F6", Action::PreviousPane),
        ],
    ),
    (
        Group::Terminal,
        &[
            ("Ctrl+Shift+C, Ctrl+Ins", Action::TerminalCopy),
            ("Ctrl+V, Ctrl+Shift+V, Shift+Ins", Action::TerminalPaste),
            ("Shift+PgUp, Shift+PgDn", Action::ScrollHistory),
            ("Ctrl+Shift+F", Action::FindInTerminal),
            ("Ctrl++, Ctrl+-, Ctrl+0", Action::TextSize),
            ("Ctrl+Alt+B", Action::Broadcast),
        ],
    ),
    (
        Group::Files,
        &[
            ("Ctrl+C, Ctrl+X", Action::FilesCopyCut),
            ("Ctrl+V", Action::FilesPaste),
            ("Ctrl+A", Action::FilesSelectAll),
            ("Ctrl+Shift+C", Action::FilesCopyPath),
            ("Ctrl+Shift+D, Ctrl+Shift+U", Action::FilesDownloadUpload),
            ("F2", Action::FilesRename),
            ("F7", Action::FilesNewFolder),
            ("Del", Action::FilesDelete),
            ("F5", Action::FilesRefresh),
            ("Backspace, Alt+Left, Alt+Up", Action::FilesBack),
            ("F4, Alt+D", Action::FilesPath),
            ("Tab", Action::FilesSwitchPane),
        ],
    ),
    (
        Group::Window,
        &[
            ("F11", Action::FullScreen),
            ("Ctrl+Alt+Home", Action::ReleaseDesktop),
            ("Ctrl+,", Action::Settings),
            ("Ctrl+Shift+S", Action::Screenshot),
            ("Ctrl+Shift+A", Action::CopyStatus),
            ("Ctrl+L", Action::Lock),
            ("F1", Action::Help),
            ("Esc", Action::Close),
        ],
    ),
];

fn group_title(group: Group) -> String {
    match group {
        Group::Sessions => fl!("ui-shortcuts-group-sessions"),
        Group::Tabs => fl!("ui-shortcuts-group-tabs"),
        Group::Terminal => fl!("ui-shortcuts-group-terminal"),
        Group::Files => fl!("ui-shortcuts-group-files"),
        Group::Window => fl!("ui-shortcuts-group-window"),
    }
}

/// What `action` does, in the window's language.
#[must_use]
pub fn action_text(action: Action) -> String {
    match action {
        Action::NewSession => fl!("ui-shortcuts-new-session"),
        Action::EditSession => fl!("ui-shortcuts-edit-session"),
        Action::QuickConnect => fl!("ui-shortcuts-quick-connect"),
        Action::Search => fl!("ui-shortcuts-search"),
        Action::UndoMove => fl!("ui-shortcuts-undo-move"),
        Action::SelectAllSessions => fl!("ui-shortcuts-select-all-sessions"),
        Action::SessionMenu => fl!("ui-shortcuts-session-menu"),
        Action::FindByName => fl!("ui-shortcuts-find-by-name"),
        Action::ToggleSidebar => fl!("ui-shortcuts-toggle-sidebar"),
        Action::NextTab => fl!("ui-shortcuts-next-tab"),
        Action::PreviousTab => fl!("ui-shortcuts-previous-tab"),
        Action::CloseTab => fl!("ui-shortcuts-close-tab"),
        Action::ToggleSplit => fl!("ui-shortcuts-toggle-split"),
        Action::NextPane => fl!("ui-shortcuts-next-pane"),
        Action::PreviousPane => fl!("ui-shortcuts-previous-pane"),
        Action::FindInTerminal => fl!("ui-shortcuts-find"),
        Action::TextSize => fl!("ui-shortcuts-text-size"),
        Action::Broadcast => fl!("ui-shortcuts-broadcast"),
        Action::TerminalCopy => fl!("ui-shortcuts-terminal-copy"),
        Action::TerminalPaste => fl!("ui-shortcuts-terminal-paste"),
        Action::ScrollHistory => fl!("ui-shortcuts-scroll-history"),
        Action::FilesCopyCut => fl!("ui-shortcuts-files-copy-cut"),
        Action::FilesPaste => fl!("ui-shortcuts-files-paste"),
        Action::FilesSelectAll => fl!("ui-shortcuts-files-select-all"),
        Action::FilesCopyPath => fl!("ui-shortcuts-files-copy-path"),
        Action::FilesDownloadUpload => fl!("ui-shortcuts-files-download-upload"),
        Action::FilesRename => fl!("ui-shortcuts-files-rename"),
        Action::FilesNewFolder => fl!("ui-shortcuts-files-new-folder"),
        Action::FilesDelete => fl!("ui-shortcuts-files-delete"),
        Action::FilesRefresh => fl!("ui-shortcuts-files-refresh"),
        Action::FilesBack => fl!("ui-shortcuts-files-back"),
        Action::FilesPath => fl!("ui-shortcuts-files-path"),
        Action::FilesSwitchPane => fl!("ui-shortcuts-files-switch-pane"),
        Action::FullScreen => fl!("ui-shortcuts-full-screen"),
        Action::ReleaseDesktop => fl!("ui-shortcuts-release-desktop"),
        Action::Settings => fl!("ui-shortcuts-settings"),
        Action::Screenshot => fl!("ui-shortcuts-screenshot"),
        Action::CopyStatus => fl!("ui-shortcuts-copy-status"),
        Action::Lock => fl!("ui-shortcuts-lock"),
        Action::Help => fl!("ui-shortcuts-help"),
        Action::Close => fl!("ui-shortcuts-close"),
    }
}

/// The dialog: its title, the keys by group, what a session keeps for itself, and `ok`.
pub fn view<'a>(ok: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let mut list = Column::new().spacing(spacing::SM);
    for (group, keys) in SHORTCUTS {
        list = list.push(text(group_title(group)).size(font_size::SUBTITLE));
        for &(keys, action) in keys {
            list = list.push(
                row![
                    text(keys)
                        .font(iced::Font::MONOSPACE)
                        .width(Length::Fixed(KEYS_WIDTH)),
                    text(action_text(action)).width(Length::Fill),
                ]
                .spacing(spacing::SM),
            );
        }
    }
    column![
        text(fl!("ui-shortcuts-title")).size(font_size::TITLE),
        styles::scroll(list).height(Length::Fixed(LIST_HEIGHT)),
        text(fl!("ui-shortcuts-session-keys")).style(text::secondary),
        row![iced::widget::space::horizontal(), ok.into()],
    ]
    .spacing(spacing::SM)
    .into()
}

/// The status bar's hint, which opens the help wherever the keyboard is: quiet, in the
/// secondary text, as the C#'s.
pub fn hint<'a>(size: f32) -> Element<'a, Message> {
    button(
        text(fl!("ui-shortcuts-hint"))
            .size(size)
            .style(text::secondary),
    )
    .style(styles::subtle)
    .padding(HINT_PADDING)
    .on_press(Message::Shortcut(
        crate::terminal_view::keys::WindowShortcut::Help,
    ))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_listed_has_its_words_and_none_twice() {
        let actions: Vec<Action> = SHORTCUTS
            .iter()
            .flat_map(|(_, keys)| keys.iter().map(|(_, action)| *action))
            .collect();
        for (index, action) in actions.iter().enumerate() {
            assert!(!actions[index + 1..].contains(action), "{action:?} twice");
            assert!(!action_text(*action).is_empty(), "{action:?}");
        }

        assert_eq!(actions.len(), 41, "every action listed");
    }
}
