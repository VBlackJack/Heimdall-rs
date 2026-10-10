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

//! What reaches a Files tab from outside its widgets: the characters typed over its lists,
//! and the files Explorer drops on its window, gathered into one drop.

use std::path::PathBuf;
use std::time::Instant;

use heimdall_app::{Effect, FilesMessage, Message as AppMessage};
use iced::{Task, window};

use super::{Message, Shell, takes_drops};
use crate::drop_batch::{self, DropPlace};

impl Shell {
    /// Characters typed that no widget took, the tree not having the keyboard: the
    /// type-ahead of the focused list of the Files tab shown, a docked SFTP pane's included,
    /// as the C# lists' WPF `TextSearch` (`EmbeddedSftpView.xaml:568-569`,
    /// `LocalFileBrowserView.xaml:110-111`). Not for Quick Connect, nor a page over the tab.
    pub(super) fn files_type_ahead(&mut self, typed: &str) -> Vec<Effect> {
        if self.palette.is_some() || self.page_over_tab() {
            return Vec::new();
        }
        let Some(tab) = self.app.active else {
            return Vec::new();
        };
        self.app.update(AppMessage::Files(FilesMessage::TypeAhead {
            tab,
            text: typed.to_owned(),
            at: Instant::now(),
        }))
    }

    /// `path` dropped on `place`: gathered with the other files of its drop, whose end is
    /// waited for when it is the first.
    pub(super) fn file_dropped(&mut self, place: DropPlace, path: PathBuf) -> Task<Message> {
        if place == DropPlace::Main {
            self.files_hovered = false;
        }
        if self.drops.add(place, path) {
            drop_batch::gather(place)
        } else {
            Task::none()
        }
    }

    /// The files dropped together on the tab's own window `window` all came: sent to its
    /// Files tab's server in one transfer. Nothing for a tab that is no Files tab, nor one
    /// without its session, nor behind the lock.
    pub(super) fn floating_drop_gathered(&mut self, window: window::Id) -> Task<Message> {
        let paths = self.take_drop(DropPlace::Floating(window));
        let Some(tab) = self
            .floating_tab_of(window)
            .filter(|tab| tab.files.is_some() && takes_drops(tab))
            .map(|tab| tab.id)
        else {
            return Task::none();
        };
        if paths.is_empty() || self.gated() {
            return Task::none();
        }
        self.apply_floating(Message::App(AppMessage::Files(FilesMessage::Dropped {
            tab,
            paths,
        })))
    }
}
