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

use heimdall_core::profile::RdpDefaults;
use heimdall_core::settings::{
    ColorScheme, Language, Settings, anti_idle_interval_accepted, max_sessions_accepted,
    rdp_auto_reconnect_attempts_accepted, rdp_connect_timeout_accepted,
    reachability_interval_accepted, reachability_probes_accepted, reachability_timeout_accepted,
    session_log_retention_days_accepted, settings_path, ssh_auto_reconnect_attempts_accepted,
    ssh_keep_alive_interval_accepted, ssh_tmout_reset_interval_accepted,
    terminal_font_size_accepted,
};
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
    /// Days a transcript is kept, 0 for every one; refused out of the C# range.
    SessionLogRetentionDays(u32),
    /// The program a server's file is edited with; empty takes the system's own.
    ExternalEditor(String),
    /// The SFTP browser's settings: on or off, and opened beside an SSH shell or not.
    SftpBrowser(heimdall_core::settings::SftpBrowser),
    /// The lists of keys trusted for servers.
    TrustedKeys(TrustedKeysMessage),
    /// The size a new terminal's text starts at; one out of the accepted range is ignored.
    TerminalFontSize(u16),
    /// The language chosen, once the window shows it.
    Language(Language),
    /// SSH auto-reconnect on or off.
    SshAutoReconnect(bool),
    /// Attempts of an SSH auto-reconnect; one out of the accepted range is ignored.
    SshAutoReconnectAttempts(u32),
    /// Attempts of an RDP auto-reconnect; one out of the accepted range is ignored.
    RdpAutoReconnectAttempts(u32),
    /// Which SSH agent's keys are offered first, or alone.
    SshAgentPreference(heimdall_core::settings::AgentPreference),
    /// Most sessions open at once, 0 for no limit.
    MaxSessions(u32),
    /// Seconds an RDP connection may take to log on, 0 for no limit.
    RdpConnectTimeout(u32),
    /// The execution policy a local `PowerShell` is started with.
    PowerShellExecutionPolicy(heimdall_core::settings::ExecutionPolicy),
    /// What Ctrl+V does in a terminal.
    CtrlVPaste(heimdall_core::settings::CtrlVPaste),
    /// Seconds between two anti-idle keys, 0 for none; refused out of the C# range.
    AntiIdleInterval(u32),
    /// Seconds between two SSH keep-alives; refused out of the C# range.
    SshKeepAliveInterval(u32),
    /// Seconds of no input before an SSH shell's `TMOUT` reset, 0 for none; refused out of
    /// the C# range.
    SshTmoutResetInterval(u32),
    /// The RDP options profiles following the application's take.
    RdpDefaults(RdpDefaults),
    /// The computer kept from sleeping while a session is open, or not.
    PreventSleep(bool),
    /// The tunnels panel starts collapsed, or open.
    CollapseTunnelsPanel(bool),
    /// The application writes its diagnostics log, or not.
    DiagnosticsLog(bool),
    /// The background check of every server runs, or not.
    Reachability(bool),
    /// Seconds between two background checks; refused out of the range.
    ReachabilityInterval(u32),
    /// Milliseconds a server has to answer it; refused out of the range.
    ReachabilityTimeout(u32),
    /// Servers it dials at once; refused out of the range.
    ReachabilityProbes(u32),
    /// The sizes the Resolution menus offer, empty for the built-in ones; refused when one
    /// is out of the limits.
    RdpResolutionPresets(Vec<(u16, u16)>),
    /// The RDP settings back to their own values, asked first as the C# asks.
    ResetRdpDefaults,
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

    /// A settings change: applied as [`App::apply_settings`] says, once agreed to when it
    /// needs to be.
    pub(super) fn settings_message(&mut self, message: &SettingsMessage) -> Vec<Effect> {
        match message {
            SettingsMessage::TrustedKeys(message) => self.trusted_keys_message(message),
            // As the C#: a transcript keeps what is typed as well as what is shown, passwords
            // echoed to the terminal included, so turning transcripts on is asked first.
            SettingsMessage::SessionLogging(true) if !self.settings.session_logging => {
                self.dialog = Some(Dialog::ConfirmSessionLogging);
                Vec::new()
            }
            SettingsMessage::ResetRdpDefaults => {
                self.dialog = Some(Dialog::ConfirmResetRdpDefaults);
                Vec::new()
            }
            _ => self.apply_settings(message),
        }
    }

    /// The RDP settings reset, as the user agreed to.
    pub(super) fn confirm_reset_rdp_defaults(&mut self) -> Vec<Effect> {
        self.apply_settings(&SettingsMessage::ResetRdpDefaults)
    }

    /// Transcripts turned on, as the user agreed to.
    pub(super) fn confirm_session_logging(&mut self) -> Vec<Effect> {
        self.apply_settings(&SettingsMessage::SessionLogging(true))
    }

    /// Sets the reachability number `message` changes, when within its range; whether it
    /// was.
    /// A limit of the settings, refused out of the C# range, which leaves it as it was;
    /// whether it was taken.
    fn set_limit(&mut self, message: &SettingsMessage) -> bool {
        match *message {
            SettingsMessage::RdpConnectTimeout(seconds)
                if rdp_connect_timeout_accepted(seconds) =>
            {
                self.settings.rdp_connect_timeout = seconds;
            }
            SettingsMessage::RdpAutoReconnectAttempts(attempts)
                if rdp_auto_reconnect_attempts_accepted(attempts) =>
            {
                self.settings.rdp_auto_reconnect_attempts = attempts;
            }
            SettingsMessage::MaxSessions(max) if max_sessions_accepted(max) => {
                self.settings.max_sessions = max;
            }
            SettingsMessage::SessionLogRetentionDays(days)
                if session_log_retention_days_accepted(days) =>
            {
                self.settings.session_log_retention_days = days;
            }
            _ => return false,
        }
        true
    }

    fn set_reachability(&mut self, message: &SettingsMessage) -> bool {
        let reachability = &mut self.settings.reachability;
        match message {
            SettingsMessage::ReachabilityInterval(seconds)
                if reachability_interval_accepted(*seconds) =>
            {
                reachability.interval = *seconds;
            }
            SettingsMessage::ReachabilityTimeout(millis)
                if reachability_timeout_accepted(*millis) =>
            {
                reachability.timeout = *millis;
            }
            SettingsMessage::ReachabilityProbes(count) if reachability_probes_accepted(*count) => {
                reachability.probes = *count;
            }
            _ => return false,
        }
        true
    }

    /// Applies `message` and saves the settings; one that cannot be saved is said and not
    /// applied. A colour scheme colours the terminals open too.
    fn apply_settings(&mut self, message: &SettingsMessage) -> Vec<Effect> {
        let before = self.settings.clone();
        match message {
            SettingsMessage::ColorScheme(scheme) => self.settings.color_scheme = *scheme,
            SettingsMessage::SessionLogging(on) => self.settings.session_logging = *on,
            SettingsMessage::SessionLogDirectory(directory) => {
                directory
                    .trim()
                    .clone_into(&mut self.settings.session_log_directory);
            }
            SettingsMessage::ExternalEditor(editor) => {
                editor.trim().clone_into(&mut self.settings.external_editor);
            }
            SettingsMessage::SftpBrowser(sftp) => self.settings.sftp_browser = *sftp,
            SettingsMessage::TerminalFontSize(size) => {
                if !terminal_font_size_accepted(*size) {
                    return Vec::new();
                }
                self.settings.terminal_font_size = *size;
            }
            SettingsMessage::Language(language) => self.settings.language = Some(*language),
            SettingsMessage::SshAutoReconnect(on) => self.settings.ssh_auto_reconnect = *on,
            SettingsMessage::PowerShellExecutionPolicy(policy) => {
                self.settings.powershell_execution_policy = *policy;
            }
            SettingsMessage::CtrlVPaste(choice) => self.settings.ctrl_v_paste = *choice,
            SettingsMessage::SshAgentPreference(preference) => {
                self.settings.ssh_agent_preference = *preference;
                // The agent chip says what the next connection reaches.
                self.agent_chip = super::agent_chip::AgentChip::Unknown;
            }
            SettingsMessage::SshAutoReconnectAttempts(attempts) => {
                if !ssh_auto_reconnect_attempts_accepted(*attempts) {
                    return Vec::new();
                }
                self.settings.ssh_auto_reconnect_attempts = *attempts;
            }
            SettingsMessage::AntiIdleInterval(seconds) => {
                if !anti_idle_interval_accepted(*seconds) {
                    return Vec::new();
                }
                self.settings.anti_idle_interval = *seconds;
            }
            SettingsMessage::SshKeepAliveInterval(seconds) => {
                if !ssh_keep_alive_interval_accepted(*seconds) {
                    return Vec::new();
                }
                self.settings.ssh_keep_alive_interval = *seconds;
            }
            SettingsMessage::SshTmoutResetInterval(seconds) => {
                if !ssh_tmout_reset_interval_accepted(*seconds) {
                    return Vec::new();
                }
                self.settings.ssh_tmout_reset_interval = *seconds;
            }
            SettingsMessage::RdpDefaults(defaults) => self.settings.rdp_defaults = *defaults,
            message @ (SettingsMessage::RdpConnectTimeout(_)
            | SettingsMessage::RdpAutoReconnectAttempts(_)
            | SettingsMessage::MaxSessions(_)
            | SettingsMessage::SessionLogRetentionDays(_)) => {
                if !self.set_limit(message) {
                    return Vec::new();
                }
            }
            SettingsMessage::PreventSleep(on) => self.settings.prevent_sleep = *on,
            SettingsMessage::CollapseTunnelsPanel(collapse) => {
                self.settings.collapse_tunnels_panel = *collapse;
            }
            SettingsMessage::DiagnosticsLog(on) => self.settings.diagnostics_log = *on,
            SettingsMessage::Reachability(on) => self.settings.reachability.enabled = *on,
            SettingsMessage::ReachabilityInterval(_)
            | SettingsMessage::ReachabilityTimeout(_)
            | SettingsMessage::ReachabilityProbes(_) => {
                if !self.set_reachability(message) {
                    return Vec::new();
                }
            }
            SettingsMessage::RdpResolutionPresets(presets) => {
                if !Settings::resolution_presets_accepted(presets) {
                    return Vec::new();
                }
                self.settings.rdp_resolution_presets.clone_from(presets);
            }
            SettingsMessage::ResetRdpDefaults => self.settings.reset_rdp(),
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
        self.reachability_changed(before.reachability)
    }
}
