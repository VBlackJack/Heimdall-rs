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

pub mod about_view;
pub mod address_test_view;
pub mod agent_chip_view;
pub mod browse;
pub mod build_date;
pub mod citrix_form;
pub mod citrix_import_view;
pub mod citrix_view;
pub mod code_editor;
pub mod column_header;
pub mod component_versions;
mod conflicts_view;
mod desktop_texture;
pub mod desktop_view;
mod detail_view;
mod dialog_parts;
pub mod drop_batch;
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
pub mod icons;
mod idle;
mod integrated_editor;
pub mod keysym;
pub mod legacy_migration_view;
pub mod local_form;
mod logging;
mod macros_view;
pub mod palette;
pub mod post_connect_form;
pub mod presets_editor;
pub mod profile_import_view;
pub mod profile_tabs;
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
mod styles;
pub mod tab_drag;
pub mod terminal_view;
mod texts;
pub mod themes;
pub mod tokens;
pub mod tools;
pub mod transcript_lines;
mod tree_drag;
mod tree_row;
pub mod tree_view;
pub mod trusted_keys_view;
pub mod tunnels_view;

use heimdall_core::paths;
use iced::{Font, Pixels, Size, window};

use crate::shell::Shell;
use crate::terminal_view::FONTS;

/// Family of the window's text on Windows: Segoe UI, as the C# Heimdall (WPF's default),
/// read from the system's fonts.
#[cfg(windows)]
const UI_FONT_FAMILY: &str = "Segoe UI";

/// Family of the window's text elsewhere: Fira Sans, embedded by iced's `fira-sans` feature.
#[cfg(not(windows))]
const UI_FONT_FAMILY: &str = "Fira Sans";

/// Font of the window's text.
pub(crate) const UI_FONT: Font = Font::with_name(UI_FONT_FAMILY);

/// The application's icon, the C# `app.ico`: its 32-pixel image, as rows of RGBA pixels.
const ICON_RGBA: &[u8] = include_bytes!("../assets/icon/heimdall-32.rgba");

/// Side of the icon's square image, in pixels.
const ICON_SIDE: u32 = 32;

/// Window size at first start, in logical pixels.
const WINDOW_SIZE: Size = Size::new(1280.0, 800.0);

/// Smallest window size, in logical pixels.
const MIN_WINDOW_SIZE: Size = Size::new(640.0, 400.0);

/// Largest side a window kept is opened at, in logical pixels: no screen is larger.
const MAX_WINDOW_SIDE: f32 = 16_384.0;

/// The icon of every window, in its title bar and the taskbar.
pub(crate) fn window_icon() -> Option<window::Icon> {
    window::icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIDE, ICON_SIDE).ok()
}

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
    // The folders restricted to the user, the Administrators and SYSTEM before anything is
    // written in them, as the C# `AclEnforcer`; here, not beside the start, so that no file
    // is made with the wider access first. A folder left as it was is said once the log is
    // open, and the application runs on.
    let unrestricted = heimdall_core::folder_acl::restrict_app_folders();
    logging::init(paths::log_dir());
    for folder in &unrestricted {
        log::warn!("{folder}");
    }
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
        icon: window_icon(),
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
        let started = shell.start_tasks();
        (
            shell,
            iced::Task::batch([opened.then(move |id| screens::restore(id, &left)), started]),
        )
    };
    let daemon = iced::daemon(boot, Shell::step, Shell::window_view)
        .title(Shell::window_title)
        .theme(|shell: &Shell, _window| shell.theme())
        .subscription(Shell::subscription)
        // The window's text at the C# body size, which a view sizes from.
        .settings(iced::Settings {
            default_text_size: Pixels(tokens::font_size::BODY),
            ..iced::Settings::default()
        })
        .default_font(UI_FONT);
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
