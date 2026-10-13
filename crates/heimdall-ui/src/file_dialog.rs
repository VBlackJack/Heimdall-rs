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

//! The system's open, save and folder dialogs, one shape for every place that asks for a
//! path.
//!
//! On Windows and macOS they are `rfd`'s: the system's own dialogs, with no library beyond
//! the system's. On Linux they are the desktop portal's `FileChooser`, spoken to over the
//! D-Bus session bus by [`portal`], in Rust: no `libdbus`, no GTK, and no program started in
//! its place. Where no portal answers, the dialog says why instead of opening.

#[cfg(target_os = "linux")]
pub mod portal;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use heimdall_app::Message as AppMessage;

use crate::i18n::fl;
use crate::shell::Message;

/// What a dialog gives back: the path or paths picked, `None` when it was cancelled, or why
/// no dialog could be shown.
pub type Picked<T> = Result<Option<T>, Unavailable>;

/// A dialog opened, its answer once it closes.
pub type Pick<T> = Pin<Box<dyn Future<Output = Picked<T>> + Send>>;

/// Why no dialog was shown, or why its answer cannot be used. Only the Linux portal fails
/// this way: the dialogs of Windows and macOS always open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// The D-Bus session bus cannot be reached, for this reason.
    NoSessionBus(String),
    /// No desktop portal, or none with a `FileChooser`, answers on the bus.
    NoPortal(String),
    /// The portal refused the request, or its answer could not be read, for this reason.
    Failed(String),
    /// The bus went away before the dialog answered.
    Closed,
    /// The dialog ended without an answer, with this response code.
    Ended(u32),
    /// The dialog said a file was chosen but named none.
    NoLocation,
    /// The place chosen is not a file of this computer: its URI.
    NotLocal(String),
}

impl Unavailable {
    /// What the user is told, in their language.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NoSessionBus(reason) => {
                fl!("ui-file-dialog-no-session-bus", reason = reason.as_str())
            }
            Self::NoPortal(reason) => fl!("ui-file-dialog-no-portal", reason = reason.as_str()),
            Self::Failed(reason) => fl!("ui-file-dialog-failed", reason = reason.as_str()),
            Self::Closed => fl!("ui-file-dialog-closed"),
            Self::Ended(code) => fl!("ui-file-dialog-ended", code = code.to_string()),
            Self::NoLocation => fl!("ui-file-dialog-no-location"),
            Self::NotLocal(uri) => fl!("ui-file-dialog-not-local", uri = uri.as_str()),
        }
    }
}

/// The window's message telling, in the status bar, why no dialog was shown.
#[must_use]
pub fn failed(unavailable: &Unavailable) -> Message {
    Message::App(AppMessage::FileDialogFailed(unavailable.message()))
}

/// A dialog being set up: its title, filters, folder, file name and window.
#[derive(Clone)]
pub struct FileDialog {
    #[cfg(not(target_os = "linux"))]
    inner: rfd::AsyncFileDialog,
    #[cfg(target_os = "linux")]
    inner: portal::Request,
}

impl Default for FileDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDialog {
    /// A dialog with the system's title, every file shown, where the system opens it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_os = "linux"))]
            inner: rfd::AsyncFileDialog::new(),
            #[cfg(target_os = "linux")]
            inner: portal::Request::default(),
        }
    }

    /// Titled `title`.
    #[must_use]
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner = self.inner.set_title(title);
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.title = title.into();
        }
        self
    }

    /// Offering the files of `extensions`, without their dot, under `name`; `*` is every
    /// file. Filters are offered in the order they are added.
    #[must_use]
    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner = self.inner.add_filter(name, extensions);
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.filters.push(portal::Filter {
                name: name.into(),
                extensions: extensions.iter().map(ToString::to_string).collect(),
            });
        }
        self
    }

    /// Opening in `folder`.
    #[must_use]
    pub fn set_directory(mut self, folder: impl AsRef<Path>) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner = self.inner.set_directory(folder);
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.current_folder = Some(folder.as_ref().to_path_buf());
        }
        self
    }

    /// The file name a save dialog offers.
    #[must_use]
    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner = self.inner.set_file_name(name);
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.current_name = Some(name.into());
        }
        self
    }

    /// Held by `parent`: in front of it, and modal where the system can. On Linux, the
    /// portal is told an X11 window; a Wayland surface would need the `xdg-foreign` protocol
    /// to be named, so there the dialog is not attached to the window.
    #[must_use]
    pub fn set_parent(mut self, parent: &dyn iced::window::Window) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner = self.inner.set_parent(&parent);
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.parent_window = portal::parent_window(parent);
        }
        self
    }

    /// The open dialog for one file.
    #[must_use]
    pub fn pick_file(self) -> Pick<PathBuf> {
        #[cfg(not(target_os = "linux"))]
        {
            let pick = self.inner.pick_file();
            Box::pin(async move { Ok(pick.await.map(|file| file.path().to_path_buf())) })
        }
        #[cfg(target_os = "linux")]
        {
            Box::pin(async move { first(portal::choose(self.inner, portal::Mode::OpenFile).await) })
        }
    }

    /// The open dialog for one file or more.
    #[must_use]
    pub fn pick_files(self) -> Pick<Vec<PathBuf>> {
        #[cfg(not(target_os = "linux"))]
        {
            let pick = self.inner.pick_files();
            Box::pin(async move {
                Ok(pick
                    .await
                    .map(|files| files.iter().map(|file| file.path().to_path_buf()).collect()))
            })
        }
        #[cfg(target_os = "linux")]
        {
            Box::pin(async move {
                portal::choose(self.inner, portal::Mode::OpenFiles)
                    .await
                    .map(|picked| picked.filter(|paths| !paths.is_empty()))
            })
        }
    }

    /// The folder dialog.
    #[must_use]
    pub fn pick_folder(self) -> Pick<PathBuf> {
        #[cfg(not(target_os = "linux"))]
        {
            let pick = self.inner.pick_folder();
            Box::pin(async move { Ok(pick.await.map(|folder| folder.path().to_path_buf())) })
        }
        #[cfg(target_os = "linux")]
        {
            Box::pin(
                async move { first(portal::choose(self.inner, portal::Mode::OpenFolder).await) },
            )
        }
    }

    /// The save dialog.
    #[must_use]
    pub fn save_file(self) -> Pick<PathBuf> {
        #[cfg(not(target_os = "linux"))]
        {
            let pick = self.inner.save_file();
            Box::pin(async move { Ok(pick.await.map(|file| file.path().to_path_buf())) })
        }
        #[cfg(target_os = "linux")]
        {
            Box::pin(async move { first(portal::choose(self.inner, portal::Mode::Save).await) })
        }
    }
}

/// The first path of an answer, for the dialogs that pick one.
#[cfg(target_os = "linux")]
fn first(picked: Picked<Vec<PathBuf>>) -> Picked<PathBuf> {
    picked.map(|paths| paths.and_then(|paths| paths.into_iter().next()))
}
