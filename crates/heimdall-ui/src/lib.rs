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

//! The Heimdall desktop application: the window, its views and the terminal widget.

mod about_view;
pub mod address_test_view;
pub mod agent_chip_view;
mod conflicts_view;
mod desktop_texture;
pub mod desktop_view;
pub mod editor_history;
mod editor_syntax;
pub mod export_file;
pub mod file_import_view;
mod files_view;
pub mod finder;
mod health_view;
pub mod hostkeys_view;
pub mod i18n;
mod integrated_editor;
pub mod keysym;
pub mod local_form;
mod logging;
pub mod palette;
pub mod post_connect_form;
pub mod presets_editor;
mod provider_view;
pub mod rdp_options;
pub mod rdp_view;
mod report;
mod restore_view;
pub mod route_test_view;
mod screenshot;
mod search_keys;
pub mod session_settings;
pub mod sessions_view;
pub mod shell;
mod shortcuts_view;
pub mod status_bar;
pub mod terminal_view;
mod texts;
pub mod transcript_lines;
pub mod tree_view;
pub mod trusted_keys_view;
pub mod tunnels_view;

use heimdall_core::paths;
use iced::{Font, Size, window};

use crate::shell::Shell;
use crate::terminal_view::FONTS;

/// Font of the window's text, embedded by iced's `fira-sans` feature.
const UI_FONT_FAMILY: &str = "Fira Sans";

/// Window size at first start, in logical pixels.
const WINDOW_SIZE: Size = Size::new(1280.0, 800.0);

/// Smallest window size, in logical pixels.
const MIN_WINDOW_SIZE: Size = Size::new(640.0, 400.0);

/// Runs the application until its window closes.
///
/// # Errors
///
/// When the window or the graphics backend cannot start.
pub fn run() -> iced::Result {
    logging::init(paths::log_dir());
    i18n::init();
    let application = iced::application(Shell::new, Shell::update, Shell::view)
        .title(Shell::title)
        .theme(Shell::theme)
        .subscription(Shell::subscription)
        .default_font(Font::with_name(UI_FONT_FAMILY))
        .window(window::Settings {
            size: WINDOW_SIZE,
            min_size: Some(MIN_WINDOW_SIZE),
            // Quitting with live sessions asks first; their sessions are then cancelled.
            exit_on_close_request: false,
            ..window::Settings::default()
        });
    FONTS
        .iter()
        .fold(application, |application, face| application.font(*face))
        .run()
}
