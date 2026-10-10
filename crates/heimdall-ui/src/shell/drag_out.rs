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

//! Entries of a local pane dragged out of the window, to Explorer or another application,
//! as the C# local list drags them (`LocalFileBrowserView.xaml.cs:612-630`: the paths as a
//! file drop, copy only).
//!
//! A press on a local entry held, the pointer leaving its window hands the drag to the
//! system: the shell's drag loop runs on the event loop's thread, through
//! [`window::run`], until the files are dropped or the drag given up. That loop takes the
//! release, which the window never sees: the press is forgotten when the pointer leaves,
//! so that nothing follows the pointer until the next press.
//!
//! The files dropped back on one of the application's windows during the drag are not
//! taken, nor for [`files_drag::DRAG_OUT_SETTLE`] after its end: the end comes as the
//! result of a task and those files as window events held by winit during the drag, then
//! carried as a subscription's messages, two roads with no order between them. Any drop
//! after that is taken, the same files dragged back from Explorer included.
//!
//! While the drag loop runs, winit is inside its event handler: it holds the window's
//! events, and a paint message it answers by asking for another (winit 0.30
//! `event_loop.rs`, `WM_PAINT`). The window is not redrawn during a drag held still, and
//! its thread may stay busy until the drag ends.

use std::path::PathBuf;

use heimdall_app::files::Side;
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use heimdall_dragout::{DragError, DragOutcome};
use iced::{Point, Task, window};

use super::{Message, Shell, main_window_task};
use crate::files_drag;

impl Shell {
    /// Whether the pointer moved to `at`, a press on a local entry held, is out of the
    /// window the press is followed in: the main window's size is known, a tab's own
    /// window's is not, its leaving then said by the window itself.
    pub(super) fn drag_leaves(&self, at: Point) -> bool {
        let extent = if self.files_drag_window.is_some() {
            None
        } else {
            self.window_extent
        };
        heimdall_dragout::SUPPORTED
            && self
                .files_drag
                .as_ref()
                .is_some_and(|drag| drag.from.side == Side::Local)
            && files_drag::outside(at, extent)
    }

    /// The pointer left the window, a press on a local pane's entry held: the entries
    /// chosen, the one pressed alone when it was not among them, are dragged out by the
    /// system. A press on the server's entries stays the window's own.
    pub(super) fn drag_out(&mut self) -> Task<Message> {
        if !heimdall_dragout::SUPPORTED {
            return Task::none();
        }
        let window = self.files_drag_window;
        let Some(from) = files_drag::take_out(&mut self.files_drag) else {
            return Task::none();
        };
        self.files_drag_window = None;
        // The entry pressed, not among those chosen: it alone is dragged, as the C# selects
        // it first.
        let chosen = self.local_chosen(from.tab);
        let effects = match from.index.filter(|index| !chosen.contains(index)) {
            Some(index) => self.app.update(AppMessage::Files(FilesMessage::Select {
                tab: from.tab,
                side: Side::Local,
                index,
            })),
            None => Vec::new(),
        };
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        let paths = self.local_chosen_paths(from.tab);
        if !paths.is_empty() {
            let round = self.dragged_out.start(&paths);
            let drag = move |id: window::Id| {
                let paths = paths.clone();
                window::run(id, move |window| files_drag::drag_out(window, &paths))
            };
            let task = match window {
                Some(id) => drag(id),
                None => main_window_task(self.main_window).and_then(drag),
            };
            tasks.push(task.map(move |result| Message::FilesDraggedOut(round, result)));
        }
        Task::batch(tasks)
    }

    /// The system's drag `round` of a local pane's entries ended, `result`: its failure
    /// said in the status bar; its files dropped back still not taken for
    /// [`files_drag::DRAG_OUT_SETTLE`]. The press it started from was forgotten when it
    /// started.
    pub(super) fn drag_out_ended(
        &mut self,
        round: u64,
        result: &Result<DragOutcome, DragError>,
    ) -> Task<Message> {
        let settle = Task::perform(tokio::time::sleep(files_drag::DRAG_OUT_SETTLE), move |()| {
            Message::FilesDragOutSettled(round)
        });
        let effects = match result {
            Ok(outcome) => {
                log::debug!("files dragged out: {outcome:?}");
                Vec::new()
            }
            Err(error) => {
                log::warn!("the files were not dragged out: {error}");
                files_drag::failure(error).map_or_else(Vec::new, |failure| {
                    self.app.update(AppMessage::DragOutFailed(failure))
                })
            }
        };
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        tasks.push(settle);
        Task::batch(tasks)
    }

    /// The indices of the entries chosen in the local pane of Files tab `tab`.
    fn local_chosen(&self, tab: TabId) -> Vec<usize> {
        self.app
            .tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .map(|files| files.local.chosen())
            .unwrap_or_default()
    }

    /// The full paths of the entries chosen in the local pane of Files tab `tab`, in the
    /// folder they were listed from: while another folder's listing is on its way, the
    /// entries shown are still that folder's.
    fn local_chosen_paths(&self, tab: TabId) -> Vec<PathBuf> {
        let Some(files) = self.app.tab(tab).and_then(|tab| tab.files.as_deref()) else {
            return Vec::new();
        };
        let folder = files.local.entries_folder();
        files
            .local
            .chosen()
            .into_iter()
            .filter_map(|index| files.local.entries.get(index))
            .map(|entry| folder.join(&entry.name))
            .collect()
    }
}
