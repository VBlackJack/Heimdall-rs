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

//! A Files tab in a tab's own window, as the C# file browser hosted in a floating window:
//! its keys, the files dropped on the window, a drag of its entries and its menus reach it
//! there, each naming it, never resolved through the main window's tab shown.

use heimdall_app::files::{FilesKey, Side};
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use iced::widget::{mouse_area, opaque, operation, pin};
use iced::{Element, Length, Task, window};

use super::{Message, Shell, takes_drops};
use crate::files_drag::FilesDrag;
use crate::floating_view::{FloatEvent, PaneField, field_id, files_menu_tab};
use crate::tree_view::TreeMenu;

impl Shell {
    /// A Files event of the tab's own window `window`: the files dragged over it, dropped on
    /// it, a press, a key. Nothing for a tab that is no Files tab; behind the lock, or under
    /// a dialog of the main window, which disables every window as the C# modal dialogs do,
    /// no key.
    pub(super) fn floating_files_event(
        &mut self,
        window: window::Id,
        event: FloatEvent,
    ) -> Task<Message> {
        let Some(tab) = self
            .floating_tab_of(window)
            .filter(|tab| tab.files.is_some())
            .map(|tab| tab.id)
        else {
            return Task::none();
        };
        match event {
            FloatEvent::FilesHovered(over) => {
                if let Some(floating) = self.floating.get_mut(&window) {
                    floating.hovered = over;
                }
                Task::none()
            }
            FloatEvent::FileDropped(path) => {
                if let Some(floating) = self.floating.get_mut(&window) {
                    floating.hovered = false;
                }
                let takes = self.app.tab(tab).is_some_and(takes_drops);
                if self.gated() || !takes {
                    return Task::none();
                }
                self.apply_floating(Message::App(AppMessage::Files(FilesMessage::Dropped {
                    tab,
                    path,
                })))
            }
            FloatEvent::PointerPressed => {
                self.press_in_floating(window, tab);
                Task::none()
            }
            _ if self.gated() || self.app.dialog.is_some() => Task::none(),
            FloatEvent::FilesKey(key) => self.floating_files_key(tab, key),
            FloatEvent::FindKey => self
                .floating_files_side(tab)
                .map_or_else(Task::none, |side| {
                    let field = field_id(tab, side, PaneField::Filter);
                    operation::focus(field.clone()).chain(operation::select_all(field))
                }),
            FloatEvent::Escape => self.floating_escape(tab),
            FloatEvent::CloseRequested
            | FloatEvent::Focused(_)
            | FloatEvent::Rescaled(_)
            | FloatEvent::Modifiers(_) => Task::none(),
        }
    }

    /// The pane that has the keyboard in Files tab `tab`, its lists in sight: none while
    /// its integrated editor is open.
    fn floating_files_side(&self, tab: TabId) -> Option<Side> {
        let files = self.app.tab(tab)?.files.as_deref()?;
        files.editor.is_none().then_some(files.focus)
    }

    /// `key` in the Files tab `tab` of a tab's own window, as the main window sends it to its
    /// Files tab shown; its selection then scrolled into view. Its lists hidden by its
    /// integrated editor take none.
    fn floating_files_key(&mut self, tab: TabId, key: FilesKey) -> Task<Message> {
        let Some(side) = self.floating_files_side(tab) else {
            return Task::none();
        };
        if key == FilesKey::FocusPath {
            self.edit_path(tab, side);
            return self
                .focus_next
                .take()
                .map_or_else(Task::none, operation::focus);
        }
        let task = self.apply_floating(Message::App(AppMessage::Files(FilesMessage::Key {
            tab,
            key,
        })));
        Task::batch([task, self.reveal_selection_of(tab)])
    }

    /// Escape in the Files tab `tab` of a tab's own window, as in the main window: its menu
    /// closes first, then its path bar typed in goes back to the folder shown, then its
    /// listing on its way is given up.
    fn floating_escape(&mut self, tab: TabId) -> Task<Message> {
        if self.floating_menu_open(tab) {
            self.menu = None;
            return Task::none();
        }
        if let Some((_, side)) = self.path_editing.filter(|(editing, _)| *editing == tab) {
            self.path_editing = None;
            return self.apply_floating(Message::App(AppMessage::Files(
                FilesMessage::PathCancelled { tab, side },
            )));
        }
        self.floating_files_key(tab, FilesKey::CancelLoad)
    }

    /// A press in the tab's own window `window`, showing Files tab `tab`: on one of its
    /// entries, the start of a drag of it, followed in that window.
    fn press_in_floating(&mut self, window: window::Id, tab: TabId) {
        if self.gated() {
            return;
        }
        let Some(at) = self
            .floating
            .get(&window)
            .map(|floating| floating.cursor.get())
        else {
            return;
        };
        self.files_drag = self
            .files_hover
            .filter(|spot| spot.tab == tab && spot.index.is_some())
            .map(|spot| FilesDrag::pressed(spot, at));
        self.files_drag_window = self.files_drag.as_ref().map(|_| window);
    }

    /// Whether the menu open is one of the Files pane of `tab`.
    fn floating_menu_open(&self, tab: TabId) -> bool {
        self.menu
            .as_ref()
            .and_then(|(menu, _)| files_menu_tab(menu))
            == Some(tab)
    }

    /// Closes the menu open when it is one of the Files pane of `tab`, a click beside it in
    /// its own window; a menu of the main window stays.
    pub(super) fn close_floating_menu(&mut self, tab: TabId) {
        if self.floating_menu_open(tab) {
            self.menu = None;
        }
    }

    /// Whether `menu` is one of the Files pane of a tab in a window of its own, drawn there
    /// rather than in the main window.
    pub(super) fn menu_in_floating(&self, menu: &TreeMenu) -> bool {
        files_menu_tab(menu).is_some_and(|tab| self.app.is_floating(tab))
    }

    /// The menu of the Files pane of `tab` open in its own window, where the pointer was: a
    /// click beside it closes it, as the main window's menus.
    pub(super) fn floating_menu(&self, tab: TabId) -> Option<Element<'_, Message>> {
        let (menu, at) = self
            .menu
            .as_ref()
            .filter(|(menu, _)| files_menu_tab(menu) == Some(tab))?;
        let entries = self.open_menu_entries(menu)?;
        Some(opaque(
            mouse_area(
                pin(entries)
                    .x(at.x)
                    .y(at.y)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::CloseTreeMenu)
            .on_right_press(Message::CloseTreeMenu),
        ))
    }
}
