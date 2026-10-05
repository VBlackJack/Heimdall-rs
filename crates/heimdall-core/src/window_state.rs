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

//! How the window was left, as the C# settings keep it: its size, whether it was maximized,
//! and the sidebar's width and whether it was hidden. The window's place on the screen is
//! not kept: a screen unplugged since would open it where nothing shows it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{StoreError, write_atomic};

/// Name of the file, beside the profiles.
pub const WINDOW_STATE_FILE_NAME: &str = "window-state.toml";

/// How the window was left; what is not known is left to the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Its width when last not maximized, in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Its height when last not maximized, in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// It was maximized.
    #[serde(default)]
    pub maximized: bool,
    /// The sidebar's width, as dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<f32>,
    /// The sidebar was hidden.
    #[serde(default)]
    pub sidebar_hidden: bool,
}

impl WindowState {
    /// The size kept, when both sides are, within `min` and `max` each.
    #[must_use]
    pub fn size_within(&self, min: (f32, f32), max: (f32, f32)) -> Option<(f32, f32)> {
        let (width, height) = (self.width?, self.height?);
        (width.is_finite() && height.is_finite())
            .then(|| (width.clamp(min.0, max.0), height.clamp(min.1, max.1)))
    }
}

/// The file beside `profiles_file`.
#[must_use]
pub fn state_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(WINDOW_STATE_FILE_NAME)
}

/// The state kept at `path`; the defaults when there is none or it cannot be read.
#[must_use]
pub fn load(path: &Path) -> WindowState {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keeps `state` at `path`.
///
/// # Errors
///
/// Returns [`StoreError`] when the file cannot be written.
pub fn save(path: &Path, state: &WindowState) -> Result<(), StoreError> {
    let text = toml::to_string_pretty(state)?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_is_kept_read_back_and_held_within_bounds() {
        let dir = tempfile::tempdir().expect("dir");
        let path = state_path(&dir.path().join("profiles.toml"));
        assert_eq!(load(&path), WindowState::default(), "none yet");
        let state = WindowState {
            width: Some(1500.0),
            height: Some(900.0),
            maximized: true,
            sidebar_width: Some(320.0),
            sidebar_hidden: true,
        };
        save(&path, &state).expect("save");
        assert_eq!(load(&path), state);
        assert_eq!(
            state.size_within((640.0, 400.0), (1400.0, 1000.0)),
            Some((1400.0, 900.0))
        );
        assert_eq!(
            WindowState::default().size_within((1.0, 1.0), (2.0, 2.0)),
            None
        );
        fs::write(&path, "width = \"wide\"").expect("write");
        assert_eq!(
            load(&path),
            WindowState::default(),
            "unreadable: the defaults"
        );
    }
}
