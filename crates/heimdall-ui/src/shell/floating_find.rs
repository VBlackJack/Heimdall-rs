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

//! The search bar over a terminal in a tab's own window, as the C# view hosted in the
//! floating window has it: Ctrl+Shift+F opens and closes it there, Escape closes it, and it
//! searches that tab alone. It is the window's own: the main window's bar, and the tab it
//! searches, are left as they are.

use iced::widget::operation;
use iced::{Task, window};

use super::{Message, Shell, shows_terminal};
use crate::finder::{Finder, field_id};

impl Shell {
    /// Ctrl+Shift+F in the tab's own window `window`: the search bar over its terminal
    /// opened, its field given the keyboard, or closed when open. Nothing behind the lock,
    /// under a dialog of the main window, or for a tab showing no terminal.
    pub(super) fn toggle_floating_finder(&mut self, window: window::Id) -> Task<Message> {
        if self.gated() || self.app.dialog.is_some() {
            return Task::none();
        }
        let Some(tab) = self
            .floating_tab_of(window)
            .filter(|tab| shows_terminal(tab))
            .map(|tab| tab.id)
        else {
            return Task::none();
        };
        let Some(floating) = self.floating.get_mut(&window) else {
            return Task::none();
        };
        if floating.finder.take().is_some() {
            return Task::none();
        }
        floating.finder = Some(Finder::new(tab));
        operation::focus(field_id(tab))
    }

    /// Escape in the tab's own window `window`: its search bar closed when open; whether it
    /// was.
    pub(super) fn close_floating_finder(&mut self, window: window::Id) -> bool {
        self.floating
            .get_mut(&window)
            .and_then(|floating| floating.finder.take())
            .is_some()
    }
}
