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

//! Files dropped from Explorer gathered into one drop.
//!
//! The C# view gets every file of a drop at once (`EmbeddedSftpView.xaml.cs:2124`) and
//! uploads them in one batch (`EmbeddedSftpView.xaml.cs:2150`), with one question for
//! whatever is in their way. winit reports the same drop as one `FileDropped` event per
//! file, sent one after the other from the system's single drop callback (on Windows,
//! `IDropTarget::Drop` walks the list dropped), and nothing after the last one says the list
//! ended.
//!
//! The files are therefore gathered from the first one for [`DROP_GATHER`], then sent on
//! together. A delay rather than the next turn of the event loop: the events reach the
//! window as messages of a subscription, carried through the executor, so no turn the window
//! sees is sure to come after the last of them. Counting the files hovered before the drop
//! would end it sooner, but would rest on winit reporting the same list twice, in that
//! order, which it promises nowhere. The events of one drop are microseconds apart, and two
//! drops by hand are seconds apart: the delay is far from both.

use std::path::PathBuf;
use std::time::Duration;

use iced::{Task, window};

use crate::shell::Message;

/// How long the files of a drop are gathered from the first one before they are sent on.
pub const DROP_GATHER: Duration = Duration::from_millis(100);

/// Where files were dropped: the main window, or a tab's own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPlace {
    /// The main window: the Files tab shown, else an RDP file's import.
    Main,
    /// A tab's own window: its Files tab.
    Floating(window::Id),
}

/// The files of the drops not sent on yet, by the window they were dropped on.
#[derive(Debug, Default)]
pub struct DropBatches {
    /// Each window's files, in the order dropped.
    open: Vec<(DropPlace, Vec<PathBuf>)>,
}

impl DropBatches {
    /// `path` dropped on `place`: whether it starts a drop there, to end after
    /// [`DROP_GATHER`].
    pub fn add(&mut self, place: DropPlace, path: PathBuf) -> bool {
        if let Some((_, paths)) = self.open.iter_mut().find(|(open, _)| *open == place) {
            paths.push(path);
            return false;
        }
        self.open.push((place, vec![path]));
        true
    }

    /// The drop on `place` ended: its files, in the order dropped.
    pub fn take(&mut self, place: DropPlace) -> Vec<PathBuf> {
        match self.open.iter().position(|(open, _)| *open == place) {
            Some(at) => self.open.remove(at).1,
            None => Vec::new(),
        }
    }
}

/// The end of the drop on `place` started now, said [`DROP_GATHER`] later.
pub fn gather(place: DropPlace) -> Task<Message> {
    Task::perform(tokio::time::sleep(DROP_GATHER), move |()| {
        Message::DropGathered(place)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_of_one_drop_are_gathered_by_window_in_their_order() {
        let mut batches = DropBatches::default();
        let other = DropPlace::Floating(window::Id::unique());
        assert!(
            batches.add(DropPlace::Main, PathBuf::from("a")),
            "starts one"
        );
        assert!(
            !batches.add(DropPlace::Main, PathBuf::from("b")),
            "joins it"
        );
        assert!(
            batches.add(other, PathBuf::from("c")),
            "another window's own"
        );
        assert_eq!(
            batches.take(DropPlace::Main),
            [PathBuf::from("a"), PathBuf::from("b")]
        );
        assert!(batches.take(DropPlace::Main).is_empty(), "sent on once");
        assert!(
            batches.add(DropPlace::Main, PathBuf::from("d")),
            "the next drop starts another"
        );
        assert_eq!(batches.take(other), [PathBuf::from("c")]);
    }
}
