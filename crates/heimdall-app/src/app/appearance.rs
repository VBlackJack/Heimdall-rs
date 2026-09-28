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

//! What the Settings page changes: the terminals' colour scheme, applied to every terminal,
//! those open included, and the session logging; saved beside the profiles.

use std::path::PathBuf;

use heimdall_core::settings::{ColorScheme, Settings, settings_path};
use heimdall_term::Palette;

use super::{App, AppConfig, Dialog, Effect, RECOVERY_EXTENSION, TrustedKeysMessage};

/// A change from the Settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMessage {
    /// The terminals' colours.
    ColorScheme(ColorScheme),
    /// Session logging on or off.
    SessionLogging(bool),
    /// The folder transcripts go to.
    SessionLogDirectory(String),
    /// The lists of keys trusted for servers.
    TrustedKeys(TrustedKeysMessage),
}

/// The colours of `scheme`.
#[must_use]
pub fn palette(scheme: ColorScheme) -> Palette {
    match scheme {
        ColorScheme::Standard => Palette::standard(),
        ColorScheme::Dracula => Palette::dracula(),
        ColorScheme::SolarizedDark => Palette::solarized_dark(),
        ColorScheme::Monokai => Palette::monokai(),
        ColorScheme::Nord => Palette::nord(),
    }
}

/// The settings beside the profiles of `config`, the file they are saved to, and the
/// dialog to show at start: `dialog`, or else the reason the settings could not be read.
/// Unreadable, the defaults are used and saved beside the file, never over it.
pub(super) fn load_settings(
    config: &AppConfig,
    dialog: Option<Dialog>,
) -> (Settings, PathBuf, Option<Dialog>) {
    let path = settings_path(&config.profiles_file);
    match Settings::load(&path) {
        Ok(settings) => (settings, path, dialog),
        Err(error) => (
            Settings::default(),
            path.with_extension(RECOVERY_EXTENSION),
            dialog.or(Some(Dialog::StoreError {
                detail: error.to_string(),
            })),
        ),
    }
}

impl App {
    /// What the Settings page changes.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The colours a new terminal draws with.
    pub(super) fn terminal_palette(&self) -> Palette {
        palette(self.settings.color_scheme)
    }

    /// Applies `message` and saves the settings; one that cannot be saved is said and not
    /// applied. A colour scheme colours the terminals open too.
    pub(super) fn settings_message(&mut self, message: &SettingsMessage) -> Vec<Effect> {
        if let SettingsMessage::TrustedKeys(message) = message {
            return self.trusted_keys_message(message);
        }
        let before = self.settings.clone();
        match message {
            SettingsMessage::ColorScheme(scheme) => self.settings.color_scheme = *scheme,
            SettingsMessage::SessionLogging(on) => self.settings.session_logging = *on,
            SettingsMessage::SessionLogDirectory(directory) => {
                directory
                    .trim()
                    .clone_into(&mut self.settings.session_log_directory);
            }
            SettingsMessage::TrustedKeys(_) => {}
        }
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings = before;
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return Vec::new();
        }
        let palette = palette(self.settings.color_scheme);
        for tab in &mut self.tabs {
            tab.terminal.set_palette(palette);
        }
        Vec::new()
    }
}
