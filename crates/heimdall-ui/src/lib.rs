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
pub mod citrix_form;
pub mod citrix_import_view;
pub mod code_editor;
pub mod column_header;
mod conflicts_view;
mod desktop_texture;
pub mod desktop_view;
mod detail_view;
pub mod editor_history;
mod editor_syntax;
pub mod export_file;
pub mod file_import_view;
pub mod files_drag;
mod files_view;
pub mod finder;
pub mod floating_view;
mod gateways_view;
mod health_view;
pub mod hostkeys_view;
pub mod i18n;
mod idle;
mod integrated_editor;
pub mod keysym;
pub mod local_form;
mod logging;
mod macros_view;
pub mod palette;
pub mod post_connect_form;
pub mod presets_editor;
mod provider_view;
pub mod rdp_options;
pub mod rdp_view;
mod report;
mod restore_view;
pub mod route_test_view;
mod screens;
mod screenshot;
mod search_keys;
pub mod session_settings;
pub mod sessions_view;
mod settings_file;
pub mod settings_rows;
pub mod shell;
mod shortcuts_view;
mod single_instance;
mod sleep_guard;
pub mod split_view;
pub mod status_bar;
pub mod tab_drag;
pub mod terminal_view;
mod texts;
pub mod themes;
pub mod transcript_lines;
mod tree_drag;
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

/// Largest side a window kept is opened at, in logical pixels: no screen is larger.
const MAX_WINDOW_SIDE: f32 = 16_384.0;

/// Runs the application until it exits: its main window opened first, put back where it
/// was left.
///
/// A daemon rather than an application, so that windows beside the main one can open; it
/// ends through `iced::exit`, as every quit already did, and when the main window closes.
/// A launch finding another instance on the configuration folder brings it forward and
/// ends without reading a file, as the C# hands over.
///
/// # Errors
///
/// When the window or the graphics backend cannot start.
pub fn run() -> iced::Result {
    logging::init(paths::log_dir());
    let single_instance::Start::Run(instance) =
        single_instance::claim(paths::config_dir().as_deref())
    else {
        return Ok(());
    };
    let watched = instance.as_ref().map(|guard| guard.dir().to_owned());
    i18n::init();
    // The connection files Remote Desktop Connection was given by a run that ended before
    // removing them, swept beside the start.
    std::thread::spawn(heimdall_app::rdp_external::sweep_stale);
    // As the window was left: its place and size, and maximized or not. Its place kept, it
    // opens hidden, to be put back there and shown.
    let left = paths::profiles_file()
        .map(|file| {
            heimdall_core::window_state::load(&heimdall_core::window_state::state_path(&file))
        })
        .unwrap_or_default();
    let size = left
        .size_within(
            (MIN_WINDOW_SIZE.width, MIN_WINDOW_SIZE.height),
            (MAX_WINDOW_SIDE, MAX_WINDOW_SIDE),
        )
        .map_or(WINDOW_SIZE, |(width, height)| Size::new(width, height));
    let hidden = screens::opens_hidden(&left);
    let maximized = left.maximized && !hidden;
    let settings = window::Settings {
        size,
        maximized,
        visible: !hidden,
        min_size: Some(MIN_WINDOW_SIZE),
        // Quitting with live sessions asks first; their sessions are then cancelled.
        exit_on_close_request: false,
        ..window::Settings::default()
    };
    // The main window asked to open, named to the shell, then put back where it was left
    // once open, as an application's boot task runs once its window is.
    let boot = move || {
        let (main, opened) = window::open(settings.clone());
        let mut shell = Shell::new();
        shell.set_main_window(main);
        if let Some(dir) = &watched {
            shell.watch_instance(dir.clone());
        }
        let left = left.clone();
        (shell, opened.then(move |id| screens::restore(id, &left)))
    };
    let daemon = iced::daemon(boot, Shell::step, Shell::window_view)
        .title(Shell::window_title)
        .theme(|shell: &Shell, _window| shell.theme())
        .subscription(Shell::subscription)
        .default_font(Font::with_name(UI_FONT_FAMILY));
    let ran = FONTS
        .iter()
        .fold(daemon, |daemon, face| daemon.font(*face))
        .run();
    // The X server started for X11 forwarding, if one was, stops with the application; one
    // started elsewhere is left running.
    heimdall_app::x11_server::shared().stop();
    // The folder owned until the windows are gone, then freed for the next launch.
    drop(instance);
    ran
}
