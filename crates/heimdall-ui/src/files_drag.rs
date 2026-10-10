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

//! A drag in a Files tab's panes, as the C# Files tab's: pressed on an entry, it becomes a
//! drag once the pointer moves; where the pointer is let go, a pane or a folder in it, is
//! where the entries go.
//!
//! Entries of a local pane dragged out of the window go on as the system's drag, to
//! Explorer or another application, as the C# local list drags them
//! (`LocalFileBrowserView.xaml.cs:612-630`); the server's entries do not, as in the C#.

use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::files::{DragOutFailure, Side};
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use heimdall_dragout::{DragError, DragOutcome};
use iced::widget::mouse_area;
use iced::window::raw_window_handle::RawWindowHandle;
use iced::{Element, Point, Size, event, mouse, window};

use crate::shell::Message;

/// How far the pointer moves, held down, before a press becomes a drag, in logical pixels,
/// as the system's drag threshold.
const DRAG_THRESHOLD: f32 = 5.0;

/// Where the pointer is in a Files tab: a pane, and the entry under it if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The tab.
    pub tab: TabId,
    /// The pane.
    pub side: Side,
    /// The entry, by its place; `None` between entries.
    pub index: Option<usize>,
}

/// A press on an entry, a drag once the pointer moves.
#[derive(Debug, Clone, PartialEq)]
pub struct FilesDrag {
    /// Where the press was: on an entry.
    pub from: Spot,
    start: Point,
    /// The pointer moved far enough: it is a drag.
    pub active: bool,
    /// Where the pointer is now, in the same tab.
    pub over: Option<Spot>,
}

impl FilesDrag {
    /// A press on `from`, at `start`.
    #[must_use]
    pub fn pressed(from: Spot, start: Point) -> Self {
        Self {
            from,
            start,
            active: false,
            over: None,
        }
    }

    /// The pointer moved to `at`: whether the press just became a drag.
    pub fn moved(&mut self, at: Point) -> bool {
        if self.active || self.start.distance(at) < DRAG_THRESHOLD {
            return false;
        }
        self.active = true;
        true
    }

    /// The message dropping the entries where the pointer is; `None` when it is not a drag
    /// or over no pane of its tab.
    #[must_use]
    pub fn drop_message(self) -> Option<AppMessage> {
        let onto = self
            .over
            .filter(|over| self.active && over.tab == self.from.tab)?;
        Some(AppMessage::Files(FilesMessage::DropEntries {
            tab: onto.tab,
            from: self.from.side,
            onto: onto.side,
            into: onto.index,
        }))
    }
}

/// While a press in a pane is held: where the pointer goes, and its release.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature `event::listen_with` takes"
)]
pub fn drag_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::FilesDragMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::FilesDragEnd)
        }
        // The press held, the window keeps following the pointer outside it: it left.
        iced::Event::Mouse(mouse::Event::CursorLeft) => Some(Message::FilesDragLeft),
        _ => None,
    }
}

/// `content`, at `spot`: the pointer coming over it and leaving it is said.
pub fn spot<'a>(content: impl Into<Element<'a, Message>>, at: Spot) -> Element<'a, Message> {
    mouse_area(content)
        .on_enter(Message::FilesHover(at))
        .on_exit(Message::FilesHoverLeft(at))
        .into()
}

/// Whether the pointer at `at`, in the window a drag is followed in, is out of it: before
/// its top left corner, or past `extent`, its size, when known.
#[must_use]
pub fn outside(at: Point, extent: Option<Size>) -> bool {
    at.x < 0.0 || at.y < 0.0 || extent.is_some_and(|size| at.x >= size.width || at.y >= size.height)
}

/// The press of `drag` taken out of the window, when it is one on a local pane's entry:
/// the system's drag loop takes it on, and its release, which the window never sees; the
/// window forgets it, so that nothing moves with the pointer until the next press. A
/// press on the server's entries stays: they are not dragged out.
pub fn take_out(drag: &mut Option<FilesDrag>) -> Option<Spot> {
    drag.take_if(|drag| drag.from.side == Side::Local)
        .map(|drag| drag.from)
}

/// How long, once the system's drag has ended, the files it dropped back on one of the
/// application's windows may still come. Its end and those files reach the window by two
/// roads that keep no order between them: the end as the result of a task, the files as
/// window events the window library held during the drag and hands on after it, then
/// carried as a subscription's messages. Both come within milliseconds; a drop the user
/// makes afterwards, from Explorer, takes seconds.
pub const DRAG_OUT_SETTLE: Duration = Duration::from_millis(500);

/// The entries of a drag out of the window, while it lasts and for [`DRAG_OUT_SETTLE`]
/// after: the system's drag can end back on one of the application's windows, which then
/// takes the files as dropped from Explorer; those are not taken, as the C# local list
/// refuses files dropped on it. Any drop after, of the same files included, is taken.
#[derive(Debug, Default)]
pub struct DragOutGuard {
    /// The paths dragged, as compared; empty when no drag is guarded.
    paths: Vec<PathBuf>,
    /// The number of the last drag started, so that a late end of an earlier one does not
    /// end it.
    round: u64,
}

impl DragOutGuard {
    /// `paths` start being dragged out: the number of this drag.
    pub fn start(&mut self, paths: &[PathBuf]) -> u64 {
        self.round = self.round.wrapping_add(1);
        self.paths = compared(paths);
        self.round
    }

    /// The drag `round` ended [`DRAG_OUT_SETTLE`] ago: from now on, every drop is taken.
    pub fn settled(&mut self, round: u64) {
        if round == self.round {
            self.paths.clear();
        }
    }

    /// A press: whatever is dropped from now on comes from elsewhere.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Whether `dropped`, one file of a drop, is one of the entries of the drag guarded.
    #[must_use]
    pub fn swallows(&self, dropped: &Path) -> bool {
        self.paths.contains(&path_compared(dropped))
    }
}

/// `paths` as compared: sorted, each once, and on Windows whatever their case, as its file
/// system names them.
fn compared(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut compared: Vec<PathBuf> = paths.iter().map(|path| path_compared(path)).collect();
    compared.sort();
    compared.dedup();
    compared
}

/// `path` as compared.
fn path_compared(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// Drags `paths` out of `window` as the system's drag, on the thread of the event loop,
/// which owns the window: its Win32 handle is handed to the shell as a number.
///
/// # Errors
///
/// [`DragError::NoWindow`] for a window that is no Win32 one, else those of
/// [`heimdall_dragout::drag_files`].
pub fn drag_out(window: &dyn window::Window, paths: &[PathBuf]) -> Result<DragOutcome, DragError> {
    let handle = window.window_handle().map(|handle| handle.as_raw());
    let Ok(RawWindowHandle::Win32(handle)) = handle else {
        return Err(DragError::NoWindow);
    };
    heimdall_dragout::drag_files(handle.hwnd.get(), paths)
}

/// What the status bar says of a drag out that failed with `error`; nothing for one of no
/// entry.
#[must_use]
pub fn failure(error: &DragError) -> Option<DragOutFailure> {
    match error {
        DragError::Empty => None,
        DragError::SeveralFolders => Some(DragOutFailure::SeveralFolders),
        DragError::Unsupported | DragError::NoWindow | DragError::NotSingleThreaded => {
            Some(DragOutFailure::Unavailable)
        }
        DragError::InvalidPath(path) => Some(DragOutFailure::Refused(path.display().to_string())),
        DragError::Shell { code, message } => {
            Some(DragOutFailure::Refused(if message.trim().is_empty() {
                format!("{code:#010x}")
            } else {
                message.trim().to_owned()
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code of an unspecified failure, `E_FAIL`.
    const E_FAIL: i32 = -2_147_467_259;

    #[test]
    fn the_pointer_is_out_before_the_corner_or_past_the_size() {
        let size = Some(Size::new(800.0, 600.0));
        assert!(!outside(Point::new(0.0, 0.0), size));
        assert!(!outside(Point::new(799.0, 599.0), size));
        assert!(outside(Point::new(-1.0, 10.0), size));
        assert!(outside(Point::new(10.0, -0.5), size));
        assert!(outside(Point::new(800.0, 10.0), size));
        assert!(outside(Point::new(10.0, 600.0), size));
        // Its size unknown, only the corner tells.
        assert!(!outside(Point::new(5000.0, 5000.0), None));
        assert!(outside(Point::new(-3.0, 5000.0), None));
    }

    #[test]
    fn leaving_the_window_the_press_held_is_said() {
        let left = drag_event(
            iced::Event::Mouse(mouse::Event::CursorLeft),
            event::Status::Ignored,
            window::Id::unique(),
        );
        assert!(matches!(left, Some(Message::FilesDragLeft)));
    }

    #[test]
    fn the_entries_dragged_out_are_known_back_while_the_drag_settles() {
        let folder = std::env::temp_dir().join("folder");
        let (a, b, c) = (folder.join("a.txt"), folder.join("b.txt"), folder.join("c"));
        let mut guard = DragOutGuard::default();
        assert!(!guard.swallows(&a), "nothing dragged yet");
        let round = guard.start(&[a.clone(), b.clone()]);
        assert!(guard.swallows(&a) && guard.swallows(&b), "in flight");
        assert!(!guard.swallows(&c), "only its own files");
        // Its end, and its files dropped back after it, in either order: still swallowed
        // until it settled.
        assert!(guard.swallows(&b));
        guard.settled(round);
        assert!(
            !guard.swallows(&a) && !guard.swallows(&b),
            "settled: the same files dropped from Explorer later are taken"
        );
    }

    #[test]
    fn a_press_ends_the_guard_and_a_late_end_spares_a_newer_drag() {
        let folder = std::env::temp_dir().join("folder");
        let a = folder.join("a.txt");
        let mut guard = DragOutGuard::default();
        guard.start(std::slice::from_ref(&a));
        guard.clear();
        assert!(!guard.swallows(&a), "a press forgets them");

        let first = guard.start(std::slice::from_ref(&a));
        let second = guard.start(std::slice::from_ref(&a));
        guard.settled(first);
        assert!(
            guard.swallows(&a),
            "the end of the first drag leaves the second"
        );
        guard.settled(second);
        assert!(!guard.swallows(&a));
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_entries_dragged_out_are_known_whatever_their_case() {
        let mut guard = DragOutGuard::default();
        guard.start(&[PathBuf::from(r"C:\Users\Me\notes.md")]);
        assert!(guard.swallows(Path::new(r"c:\users\me\NOTES.md")));
    }

    #[test]
    fn a_failure_is_said_by_its_kind_the_shell_in_its_own_words() {
        assert_eq!(failure(&DragError::Empty), None);
        assert_eq!(
            failure(&DragError::SeveralFolders),
            Some(DragOutFailure::SeveralFolders)
        );
        for unavailable in [
            DragError::Unsupported,
            DragError::NoWindow,
            DragError::NotSingleThreaded,
        ] {
            assert_eq!(failure(&unavailable), Some(DragOutFailure::Unavailable));
        }
        let refused = DragError::Shell {
            code: E_FAIL,
            message: "Unspecified error\r\n".to_owned(),
        };
        assert_eq!(
            failure(&refused),
            Some(DragOutFailure::Refused("Unspecified error".to_owned()))
        );
        let silent = DragError::Shell {
            code: E_FAIL,
            message: String::new(),
        };
        assert_eq!(
            failure(&silent),
            Some(DragOutFailure::Refused("0x80004005".to_owned()))
        );
    }
}
