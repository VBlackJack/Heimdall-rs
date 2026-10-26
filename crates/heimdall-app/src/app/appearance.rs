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

use heimdall_core::import::citrix_cache;
use heimdall_core::profile::{RdpDefaults, RdpMode, SshMode};
use heimdall_core::settings::{
    Accent, AppTheme, ColorScheme, Language, Settings, TerminalMinimumContrast,
    anti_idle_interval_accepted, auto_lock_idle_minutes_accepted, max_sessions_accepted,
    rdp_auto_reconnect_attempts_accepted, rdp_connect_timeout_accepted,
    rdp_default_resolution_accepted, rdp_keep_alive_interval_accepted,
    rdp_resize_enable_delay_accepted, reachability_interval_accepted, reachability_probes_accepted,
    reachability_timeout_accepted, reconnect_delay_accepted, session_log_retention_days_accepted,
    settings_path, ssh_auto_reconnect_attempts_accepted, ssh_keep_alive_interval_accepted,
    ssh_tmout_reset_interval_accepted, terminal_font_family, terminal_font_size_accepted,
    update_interval_accepted, windows_hello_grace_minutes_accepted,
    windows_hello_vault_max_days_accepted,
};
use heimdall_core::store::ProfileStore;
use heimdall_term::{MinimumContrast, Palette};

use super::{App, AppConfig, Dialog, Effect, RECOVERY_EXTENSION, TrustedKeysMessage};

/// A change from the Settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMessage {
    /// The terminals' colours.
    ColorScheme(ColorScheme),
    /// How much contrast terminal text keeps against its background.
    TerminalMinimumContrast(TerminalMinimumContrast),
    /// Session logging on or off.
    SessionLogging(bool),
    /// The folder transcripts go to.
    SessionLogDirectory(String),
    /// Days a transcript is kept, 0 for every one; refused out of the C# range.
    SessionLogRetentionDays(u32),
    /// The program a server's file is edited with; empty takes the system's own.
    ExternalEditor(String),
    /// `PuTTY`, which SSH profiles in the external mode open in; empty looks on `PATH`.
    PuttyPath(String),
    /// The `SelfService.exe` Citrix cache launches go through; empty tries the places Citrix
    /// Workspace installs it. Refused unless an absolute local path to a file of that name.
    CitrixSelfServicePath(String),
    /// The folder the Citrix Workspace cache is read from; empty is the account's own.
    /// Refused unless an absolute local path.
    CitrixCacheDirectory(String),
    /// Only the Citrix store files named `<prefix>*_Cache.xml` are read; empty reads them
    /// all. Refused unless safe file-name characters.
    CitrixCacheStorePrefix(String),
    /// Where the shell of a new SSH profile opens.
    SshDefaultMode(SshMode),
    /// The default SSH mode written into every SSH profile, asked first as the C# asks.
    ApplySshModeToAll,
    /// The X server started for X11 forwarding; empty tries the known places, then `PATH`.
    X11ServerPath(String),
    /// An X server is started when X11 forwarding needs one, or not.
    X11AutoStart(bool),
    /// The SFTP browser's settings: on or off, and opened beside an SSH shell or not.
    SftpBrowser(heimdall_core::settings::SftpBrowser),
    /// The lists of keys trusted for servers.
    TrustedKeys(TrustedKeysMessage),
    /// The size a new terminal's text starts at; one out of the accepted range is ignored.
    TerminalFontSize(u16),
    /// The family of every terminal's text, those open included; empty is the default.
    TerminalFontFamily(String),
    /// The language chosen, once the window shows it.
    Language(Language),
    /// The window's theme.
    Theme(AppTheme),
    /// The window's accent.
    Accent(Accent),
    /// SSH auto-reconnect on or off.
    SshAutoReconnect(bool),
    /// Attempts of an SSH auto-reconnect; one out of the accepted range is ignored.
    SshAutoReconnectAttempts(u32),
    /// Seconds before the first SSH auto-reconnect attempt; one out of the accepted range is
    /// ignored.
    SshAutoReconnectFirstDelay(u32),
    /// Seconds before the second attempt; one out of the accepted range is ignored.
    SshAutoReconnectSecondDelay(u32),
    /// Seconds before every attempt after the second; one out of the accepted range is
    /// ignored.
    SshAutoReconnectSubsequentDelay(u32),
    /// Attempts of an RDP auto-reconnect; one out of the accepted range is ignored.
    RdpAutoReconnectAttempts(u32),
    /// Ask before an RDP desktop is disconnected from its session bar, or not.
    RdpConfirmDisconnect(bool),
    /// Every redirection on an RDP desktop's bar, the ones not shared too, or behind "+N".
    RdpRedirectionIndicatorsExpanded(bool),
    /// Which SSH agent's keys are offered first, or alone.
    SshAgentPreference(heimdall_core::settings::AgentPreference),
    /// Most sessions open at once, 0 for no limit.
    MaxSessions(u32),
    /// Seconds an RDP connection may take to log on, 0 for no limit.
    RdpConnectTimeout(u32),
    /// Milliseconds a desktop following its tab waits after connecting, 0 for none.
    RdpResizeEnableDelay(u32),
    /// Milliseconds between two keep-alives of an embedded RDP connection; one out of the
    /// accepted range is ignored.
    RdpKeepAliveInterval(u32),
    /// Pixels of the default resolution's width; one out of the accepted range is ignored.
    RdpDefaultWidth(u32),
    /// Pixels of the default resolution's height; one out of the accepted range is ignored.
    RdpDefaultHeight(u32),
    /// The built-in RDP client offers bulk compression when the profile asks for it, or
    /// never.
    RdpBuiltinCompression(bool),
    /// The execution policy a local `PowerShell` is started with.
    PowerShellExecutionPolicy(heimdall_core::settings::ExecutionPolicy),
    /// What Ctrl+V does in a terminal.
    CtrlVPaste(heimdall_core::settings::CtrlVPaste),
    /// What Ctrl+K does in a terminal.
    CtrlKTerminal(heimdall_core::settings::CtrlKTerminal),
    /// Seconds between two anti-idle keys, 0 for none; refused out of the C# range.
    AntiIdleInterval(u32),
    /// Seconds between two SSH keep-alives; refused out of the C# range.
    SshKeepAliveInterval(u32),
    /// Seconds of no input before an SSH shell's `TMOUT` reset, 0 for none; refused out of
    /// the C# range.
    SshTmoutResetInterval(u32),
    /// The RDP options profiles following the application's take.
    RdpDefaults(RdpDefaults),
    /// Where the desktop of a new RDP profile opens.
    RdpDefaultMode(RdpMode),
    /// The default RDP mode written into every RDP profile, asked first as the C# asks.
    ApplyRdpModeToAll,
    /// The computer kept from sleeping while a session is open, or not.
    PreventSleep(bool),
    /// The tunnels panel starts collapsed, or open.
    CollapseTunnelsPanel(bool),
    /// The properties panel docked under the tree, or not; the same choice as the tree
    /// filter menu's and Ctrl+Shift+B.
    ShowPropertiesPanel(bool),
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
    /// The external tools, in their order, their names and programs trimmed as the C# Save
    /// trims them; refused, as the C# Save refuses it, when one has no name or no program or
    /// two share a name. Whether each program is there is the editor's to check.
    ExternalTools(Vec<heimdall_core::external_tools::ExternalTool>),
    /// The values of a `MobaXterm` file's `[NAME]` variables, name to login, names trimmed;
    /// refused, as the C# settings page refuses it, when a name is not one the import
    /// recognises, two names differ only by case, or a value is empty.
    MobaxtermVariables(std::collections::BTreeMap<String, String>),
    /// The shared session sources, in their order, name and path trimmed; refused, as the
    /// C# settings page and import refuse them, when a source has no name or one used twice
    /// in any case, a path that is not absolute (or does not start with a `%VARIABLE%`), or
    /// an identifier that is not valid or is used twice.
    SharedSources(Vec<heimdall_core::shared_sessions::SharedSource>),
    /// The RDP settings back to their own values, asked first as the C# asks.
    ResetRdpDefaults,
    /// Minutes without input before the workspace locks, 0 for never; refused out of the
    /// C# range.
    AutoLockIdleMinutes(u32),
    /// Locking the workspace closes every session, or leaves them running behind the lock.
    DisconnectOnLock(bool),
    /// A newer release is looked for by itself, or not.
    UpdateChecks(bool),
    /// Hours between two looks for a newer release; refused out of the C# range.
    UpdateInterval(u32),
    /// An embedded RDP session opens only while Credential Guard runs, or whenever.
    RequireCredentialGuard(bool),
    /// The user's OpenSSH `known_hosts` is imported at each start, or not.
    SyncKnownHostsAtStartup(bool),
    /// A connection waits for Windows Hello, or not.
    RequireWindowsHello(bool),
    /// Minutes a Windows Hello verification counts, 0 for every time; refused out of the
    /// C# range.
    WindowsHelloGraceMinutes(u32),
    /// Days Windows Hello unlocks the vault before the master password is asked again, 0
    /// for never; refused out of the C# range.
    VaultHelloMaxDays(u32),
    /// Every setting back to its default but what the C# keeps, asked first as the C#
    /// "Reset defaults" asks (`SettingsViewModel.cs:2376-2464`).
    ResetAllSettings,
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

/// The colours of `scheme`, text kept at least `contrast` against its background.
#[must_use]
pub fn palette_with_contrast(scheme: ColorScheme, contrast: TerminalMinimumContrast) -> Palette {
    palette(scheme).with_minimum_contrast(contrast.tenths().map(MinimumContrast::from_tenths))
}

/// The settings beside the profiles of `config`, the file they are saved to, and the
/// dialog to show at start: `dialog`, or else the reason the settings could not be read, or
/// else the files of `store` and the settings read from their last good copy, as the C# says
/// it at start. Unreadable with no usable copy, the defaults are used and saved beside the
/// file, never over it.
pub(super) fn load_settings(
    config: &AppConfig,
    dialog: Option<Dialog>,
    store: &ProfileStore,
) -> (Settings, PathBuf, Option<Dialog>) {
    let path = settings_path(&config.profiles_file);
    let (settings, file, settings_recovered, dialog) = match Settings::load_recovering(&path) {
        Ok((settings, recovered)) => (settings, path, recovered, dialog),
        Err(error) => (
            Settings::default(),
            path.with_extension(RECOVERY_EXTENSION),
            false,
            dialog.or(Some(Dialog::StoreUnreadable {
                detail: error.to_string(),
            })),
        ),
    };
    let documents: Vec<String> = [
        (store.recovered_from_backup(), store.path()),
        (settings_recovered, file.as_path()),
    ]
    .into_iter()
    .filter(|(recovered, _)| *recovered)
    .filter_map(|(_, path)| path.file_name())
    .map(|name| name.to_string_lossy().into_owned())
    .collect();
    let dialog =
        dialog.or_else(|| (!documents.is_empty()).then_some(Dialog::ConfigRecovered { documents }));
    (settings, file, dialog)
}

impl App {
    /// What the Settings page changes.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The colours a new terminal draws with.
    pub(super) fn terminal_palette(&self) -> Palette {
        palette_with_contrast(
            self.settings.color_scheme,
            self.settings.terminal_minimum_contrast,
        )
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
            SettingsMessage::ResetAllSettings => {
                self.dialog = Some(Dialog::ConfirmResetAllSettings);
                Vec::new()
            }
            SettingsMessage::ApplySshModeToAll => {
                self.ask_apply_ssh_mode();
                Vec::new()
            }
            SettingsMessage::ApplyRdpModeToAll => {
                self.ask_apply_rdp_mode();
                Vec::new()
            }
            _ => self.apply_settings(message),
        }
    }

    /// Asks before the default SSH mode is written into every SSH profile, saying how many
    /// change of how many; nothing is asked when none would, as the C# logs "no changes
    /// needed" and stops.
    fn ask_apply_ssh_mode(&mut self) {
        let mode = self.settings.ssh_default_mode;
        let (changes, total) = self.store.ssh_mode_changes(mode);
        if changes > 0 {
            self.dialog = Some(Dialog::ConfirmApplySshMode {
                mode,
                changes,
                total,
            });
        }
    }

    /// `mode` written into every SSH profile, as the user agreed to: one save, after which
    /// it is the default too, as the C# saves it in the same gesture. A save that fails
    /// leaves every profile as it was, and says why.
    pub(super) fn confirm_apply_ssh_mode(&mut self, mode: SshMode) -> Vec<Effect> {
        if let Err(error) = self.store.apply(|store| store.set_ssh_modes(mode)) {
            self.dialog = Some(Dialog::save_failed(&error));
            return Vec::new();
        }
        if self.settings.ssh_default_mode == mode {
            return Vec::new();
        }
        self.apply_settings(&SettingsMessage::SshDefaultMode(mode))
    }

    /// Asks before the default RDP mode is written into every RDP profile, saying how many
    /// change of how many, as the C# `ApplyRdpModeToAll` (`SettingsViewModel.cs:1627-1638`,
    /// through `ApplyModeToAllAsync` at 1661-1689); nothing is asked when none would, as the
    /// C# logs "no changes needed" and stops.
    fn ask_apply_rdp_mode(&mut self) {
        let mode = self.settings.rdp_default_mode;
        let (changes, total) = self.store.rdp_mode_changes(mode);
        if changes > 0 {
            self.dialog = Some(Dialog::ConfirmApplyRdpMode {
                mode,
                changes,
                total,
            });
        }
    }

    /// `mode` written into every RDP profile, as the user agreed to: one save, after which
    /// it is the default too, as the C# saves it in the same gesture. A save that fails
    /// leaves every profile as it was, and says why.
    pub(super) fn confirm_apply_rdp_mode(&mut self, mode: RdpMode) -> Vec<Effect> {
        if let Err(error) = self.store.apply(|store| store.set_rdp_modes(mode)) {
            self.dialog = Some(Dialog::save_failed(&error));
            return Vec::new();
        }
        if self.settings.rdp_default_mode == mode {
            return Vec::new();
        }
        self.apply_settings(&SettingsMessage::RdpDefaultMode(mode))
    }

    /// The RDP settings reset, as the user agreed to.
    pub(super) fn confirm_reset_rdp_defaults(&mut self) -> Vec<Effect> {
        self.apply_settings(&SettingsMessage::ResetRdpDefaults)
    }

    /// Every setting reset, as the user agreed to; what the Settings page held typed goes
    /// with it. A save that fails leaves every setting as it was.
    pub(super) fn confirm_reset_all_settings(&mut self) -> Vec<Effect> {
        let mut effects = self.apply_settings(&SettingsMessage::ResetAllSettings);
        // A save that failed says so in a dialog of its own.
        if self.dialog.is_none() {
            // The remembered passwords of the shared sessions are not a setting, yet a reset
            // to defaults leaves nothing remembered behind.
            self.purge_shared_passwords();
            effects.push(Effect::SettingsReset);
        }
        effects
    }

    /// Transcripts turned on, as the user agreed to.
    pub(super) fn confirm_session_logging(&mut self) -> Vec<Effect> {
        self.apply_settings(&SettingsMessage::SessionLogging(true))
    }

    /// A limit of the settings, refused out of the C# range, which leaves it as it was;
    /// whether it was taken.
    fn set_limit(&mut self, message: &SettingsMessage) -> bool {
        match *message {
            SettingsMessage::RdpConnectTimeout(seconds)
                if rdp_connect_timeout_accepted(seconds) =>
            {
                self.settings.rdp_connect_timeout = seconds;
            }
            SettingsMessage::RdpResizeEnableDelay(ms) if rdp_resize_enable_delay_accepted(ms) => {
                self.settings.rdp_resize_enable_delay_ms = ms;
            }
            SettingsMessage::RdpDefaultWidth(width) if rdp_default_resolution_accepted(width) => {
                self.settings.rdp_default_width = width;
            }
            SettingsMessage::RdpDefaultHeight(height)
                if rdp_default_resolution_accepted(height) =>
            {
                self.settings.rdp_default_height = height;
            }
            SettingsMessage::RdpKeepAliveInterval(ms) if rdp_keep_alive_interval_accepted(ms) => {
                self.settings.rdp_keep_alive_interval_ms = ms;
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
            SettingsMessage::AutoLockIdleMinutes(minutes)
                if auto_lock_idle_minutes_accepted(minutes) =>
            {
                self.settings.auto_lock_idle_minutes = minutes;
            }
            SettingsMessage::SshAutoReconnectAttempts(attempts)
                if ssh_auto_reconnect_attempts_accepted(attempts) =>
            {
                self.settings.ssh_auto_reconnect_attempts = attempts;
            }
            SettingsMessage::SshAutoReconnectFirstDelay(seconds)
                if reconnect_delay_accepted(seconds) =>
            {
                self.settings.ssh_auto_reconnect_first_delay = seconds;
            }
            SettingsMessage::SshAutoReconnectSecondDelay(seconds)
                if reconnect_delay_accepted(seconds) =>
            {
                self.settings.ssh_auto_reconnect_second_delay = seconds;
            }
            SettingsMessage::SshAutoReconnectSubsequentDelay(seconds)
                if reconnect_delay_accepted(seconds) =>
            {
                self.settings.ssh_auto_reconnect_subsequent_delay = seconds;
            }
            SettingsMessage::AntiIdleInterval(seconds) if anti_idle_interval_accepted(seconds) => {
                self.settings.anti_idle_interval = seconds;
            }
            SettingsMessage::SshKeepAliveInterval(seconds)
                if ssh_keep_alive_interval_accepted(seconds) =>
            {
                self.settings.ssh_keep_alive_interval = seconds;
            }
            SettingsMessage::SshTmoutResetInterval(seconds)
                if ssh_tmout_reset_interval_accepted(seconds) =>
            {
                self.settings.ssh_tmout_reset_interval = seconds;
            }
            SettingsMessage::UpdateInterval(hours) if update_interval_accepted(hours) => {
                self.settings.updates.interval_hours = hours;
            }
            SettingsMessage::WindowsHelloGraceMinutes(minutes)
                if windows_hello_grace_minutes_accepted(minutes) =>
            {
                self.settings.windows_hello.grace_minutes = minutes;
            }
            SettingsMessage::VaultHelloMaxDays(days)
                if windows_hello_vault_max_days_accepted(days) =>
            {
                self.settings.windows_hello.vault_max_days = days;
            }
            _ => return false,
        }
        true
    }

    /// Sets what `message` changes of the outside programs and folders: the session log
    /// folder, the external editor, `PuTTY`, the default SSH and RDP modes and the X server.
    fn set_external_clients(&mut self, message: &SettingsMessage) {
        let settings = &mut self.settings;
        match message {
            SettingsMessage::SessionLogDirectory(directory) => {
                directory
                    .trim()
                    .clone_into(&mut settings.session_log_directory);
            }
            SettingsMessage::ExternalEditor(editor) => {
                editor.trim().clone_into(&mut settings.external_editor);
            }
            SettingsMessage::PuttyPath(path) => path.trim().clone_into(&mut settings.putty_path),
            SettingsMessage::SshDefaultMode(mode) => settings.ssh_default_mode = *mode,
            SettingsMessage::RdpDefaultMode(mode) => settings.rdp_default_mode = *mode,
            SettingsMessage::X11ServerPath(path) => {
                path.trim().clone_into(&mut settings.x11_server_path);
            }
            SettingsMessage::X11AutoStart(on) => settings.x11_auto_start = *on,
            _ => {}
        }
    }

    /// Sets the list `message` changes, the resolution presets or the external tools, when
    /// it is accepted; whether it was. The external tools are refused as the C# Save refuses
    /// them, one without a name or a program or two of one name; whether each program is
    /// there is the editor's to check.
    fn set_list(&mut self, message: &SettingsMessage) -> bool {
        use heimdall_core::external_tools::{problem, saved};
        match message {
            SettingsMessage::RdpResolutionPresets(presets)
                if Settings::resolution_presets_accepted(presets) =>
            {
                self.settings.rdp_resolution_presets.clone_from(presets);
            }
            SettingsMessage::ExternalTools(tools) if problem(tools, |_| true).is_none() => {
                self.settings.external_tools = saved(tools);
            }
            SettingsMessage::MobaxtermVariables(variables) if variables_accepted(variables) => {
                self.settings.mobaxterm_variables = variables
                    .iter()
                    .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
                    .collect();
            }
            SettingsMessage::SharedSources(sources) if shared_sources_accepted(sources) => {
                self.settings.shared_sources = sources
                    .iter()
                    .map(|source| heimdall_core::shared_sessions::SharedSource {
                        id: source.id.clone(),
                        name: source.name.trim().to_owned(),
                        path: source.path.trim().to_owned(),
                        enabled: source.enabled,
                    })
                    .collect();
            }
            _ => return false,
        }
        true
    }

    /// Sets the reachability number `message` changes, when within its range; whether it
    /// was.
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
    /// applied. A colour scheme or a minimum contrast colours the terminals open too.
    #[expect(clippy::too_many_lines, reason = "one arm per setting")]
    fn apply_settings(&mut self, message: &SettingsMessage) -> Vec<Effect> {
        let before = self.settings.clone();
        match message {
            SettingsMessage::ColorScheme(scheme) => self.settings.color_scheme = *scheme,
            SettingsMessage::TerminalMinimumContrast(contrast) => {
                self.settings.terminal_minimum_contrast = *contrast;
            }
            SettingsMessage::SessionLogging(on) => self.settings.session_logging = *on,
            SettingsMessage::SessionLogDirectory(_)
            | SettingsMessage::ExternalEditor(_)
            | SettingsMessage::PuttyPath(_)
            | SettingsMessage::SshDefaultMode(_)
            | SettingsMessage::RdpDefaultMode(_)
            | SettingsMessage::X11ServerPath(_)
            | SettingsMessage::X11AutoStart(_) => self.set_external_clients(message),
            SettingsMessage::SftpBrowser(sftp) => self.settings.sftp_browser = *sftp,
            SettingsMessage::TerminalFontSize(size) => {
                if !terminal_font_size_accepted(*size) {
                    return Vec::new();
                }
                self.settings.terminal_font_size = *size;
            }
            SettingsMessage::TerminalFontFamily(family) => {
                self.settings.terminal_font_family = terminal_font_family(family);
            }
            SettingsMessage::Language(language) => self.settings.language = Some(*language),
            SettingsMessage::Theme(theme) => self.settings.theme = *theme,
            SettingsMessage::Accent(accent) => self.settings.accent = *accent,
            SettingsMessage::SshAutoReconnect(on) => self.settings.ssh_auto_reconnect = *on,
            SettingsMessage::PowerShellExecutionPolicy(policy) => {
                self.settings.powershell_execution_policy = *policy;
            }
            SettingsMessage::CtrlVPaste(choice) => self.settings.ctrl_v_paste = *choice,
            SettingsMessage::CtrlKTerminal(choice) => self.settings.ctrl_k_terminal = *choice,
            SettingsMessage::SshAgentPreference(preference) => {
                self.settings.ssh_agent_preference = *preference;
                // The agent chip says what the next connection reaches.
                self.agent_chip = super::agent_chip::AgentChip::Unknown;
            }
            SettingsMessage::RdpDefaults(defaults) => self.settings.rdp_defaults = *defaults,
            message @ (SettingsMessage::RdpConnectTimeout(_)
            | SettingsMessage::RdpResizeEnableDelay(_)
            | SettingsMessage::RdpKeepAliveInterval(_)
            | SettingsMessage::RdpDefaultWidth(_)
            | SettingsMessage::RdpDefaultHeight(_)
            | SettingsMessage::RdpAutoReconnectAttempts(_)
            | SettingsMessage::MaxSessions(_)
            | SettingsMessage::SessionLogRetentionDays(_)
            | SettingsMessage::AutoLockIdleMinutes(_)
            | SettingsMessage::SshAutoReconnectAttempts(_)
            | SettingsMessage::SshAutoReconnectFirstDelay(_)
            | SettingsMessage::SshAutoReconnectSecondDelay(_)
            | SettingsMessage::SshAutoReconnectSubsequentDelay(_)
            | SettingsMessage::AntiIdleInterval(_)
            | SettingsMessage::SshKeepAliveInterval(_)
            | SettingsMessage::SshTmoutResetInterval(_)
            | SettingsMessage::UpdateInterval(_)
            | SettingsMessage::WindowsHelloGraceMinutes(_)
            | SettingsMessage::VaultHelloMaxDays(_)) => {
                if !self.set_limit(message) {
                    return Vec::new();
                }
            }
            message @ (SettingsMessage::DisconnectOnLock(_)
            | SettingsMessage::RequireCredentialGuard(_)
            | SettingsMessage::RequireWindowsHello(_)
            | SettingsMessage::SyncKnownHostsAtStartup(_)) => self.set_security_flag(message),
            SettingsMessage::CitrixSelfServicePath(path) => {
                match checked_citrix(path, citrix_cache::self_service_path_valid) {
                    Some(path) => {
                        self.settings.citrix_self_service_path = path;
                        self.settings
                            .citrix_ignored
                            .retain(|s| *s != citrix_cache::CitrixSetting::SelfServicePath);
                    }
                    None => return Vec::new(),
                }
            }
            SettingsMessage::CitrixCacheDirectory(directory) => {
                match checked_citrix(directory, citrix_cache::cache_directory_valid) {
                    Some(directory) => {
                        self.settings.citrix_cache_directory = directory;
                        self.settings
                            .citrix_ignored
                            .retain(|s| *s != citrix_cache::CitrixSetting::CacheDirectory);
                    }
                    None => return Vec::new(),
                }
            }
            SettingsMessage::CitrixCacheStorePrefix(prefix) => {
                let normalized = citrix_cache::normalize_store_prefix(prefix);
                // Blank clears it; anything else must be safe.
                if normalized.is_none() && !prefix.trim().is_empty() {
                    return Vec::new();
                }
                self.settings.citrix_cache_store_prefix = normalized.unwrap_or_default();
                self.settings
                    .citrix_ignored
                    .retain(|s| *s != citrix_cache::CitrixSetting::StorePrefix);
            }
            SettingsMessage::PreventSleep(on) => self.settings.prevent_sleep = *on,
            SettingsMessage::RdpBuiltinCompression(on) => {
                self.settings.builtin_rdp_compression = *on;
            }
            SettingsMessage::RdpConfirmDisconnect(on) => self.settings.rdp_confirm_disconnect = *on,
            SettingsMessage::RdpRedirectionIndicatorsExpanded(on) => {
                self.settings.rdp_redirection_indicators_expanded = *on;
            }
            SettingsMessage::ShowPropertiesPanel(on) => self.settings.show_properties_panel = *on,
            SettingsMessage::CollapseTunnelsPanel(collapse) => {
                self.settings.collapse_tunnels_panel = *collapse;
            }
            SettingsMessage::DiagnosticsLog(on) => self.settings.diagnostics_log = *on,
            SettingsMessage::UpdateChecks(on) => self.settings.updates.enabled = *on,
            SettingsMessage::Reachability(on) => self.settings.reachability.enabled = *on,
            SettingsMessage::ReachabilityInterval(_)
            | SettingsMessage::ReachabilityTimeout(_)
            | SettingsMessage::ReachabilityProbes(_) => {
                if !self.set_reachability(message) {
                    return Vec::new();
                }
            }
            message @ (SettingsMessage::RdpResolutionPresets(_)
            | SettingsMessage::ExternalTools(_)
            | SettingsMessage::MobaxtermVariables(_)
            | SettingsMessage::SharedSources(_)) => {
                if !self.set_list(message) {
                    return Vec::new();
                }
            }
            SettingsMessage::ResetRdpDefaults => self.settings.reset_rdp(),
            SettingsMessage::ResetAllSettings => {
                self.settings.reset_all();
                // The agent preference may have changed: the chip says what the next
                // connection reaches.
                self.agent_chip = super::agent_chip::AgentChip::Unknown;
            }
            SettingsMessage::TrustedKeys(_)
            | SettingsMessage::ApplySshModeToAll
            | SettingsMessage::ApplyRdpModeToAll => {}
        }
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings = before;
            self.dialog = Some(Dialog::save_failed(&error));
            return Vec::new();
        }
        let palette = self.terminal_palette();
        for tab in &mut self.tabs {
            tab.terminal.set_palette(palette);
        }
        self.settings_changed(&before)
    }

    /// Sets the security switch `message` changes; the `known_hosts` import at startup is
    /// one, as it trusts keys without asking.
    fn set_security_flag(&mut self, message: &SettingsMessage) {
        match message {
            SettingsMessage::DisconnectOnLock(on) => self.settings.disconnect_on_lock = *on,
            SettingsMessage::RequireCredentialGuard(on) => {
                self.settings.require_credential_guard = *on;
            }
            SettingsMessage::RequireWindowsHello(on) => {
                self.settings.windows_hello.require_on_connect = *on;
            }
            SettingsMessage::SyncKnownHostsAtStartup(on) => {
                self.settings.sync_known_hosts_at_startup = *on;
            }
            _ => {}
        }
    }

    /// What the settings changed from `before` start: the background check of every server
    /// as it now runs, and Credential Guard checked when just required, so that the first
    /// session does not wait for it.
    fn settings_changed(&mut self, before: &Settings) -> Vec<Effect> {
        // A list of shared sources edited is followed at once, not at the timer's next look.
        if before.shared_sources != self.settings.shared_sources {
            self.shared_tick(std::time::SystemTime::now());
        }
        let mut effects = self.reachability_changed(before.reachability);
        if !before.require_credential_guard {
            effects.extend(self.warm_credential_guard());
        }
        effects
    }
}

/// `text` trimmed when it is blank (cleared) or `valid` takes it; `None` when refused.
fn checked_citrix(text: &str, valid: fn(&str) -> bool) -> Option<String> {
    let trimmed = text.trim();
    (trimmed.is_empty() || valid(trimmed)).then(|| trimmed.to_owned())
}

/// Whether `variables` can be kept, as the C# row checks
/// (`SettingsViewModel.MobaXtermVariables.cs:108`): every name is one the import recognises,
/// none repeats another in any case, and every value says something.
fn variables_accepted(variables: &std::collections::BTreeMap<String, String>) -> bool {
    use heimdall_core::import::mobaxterm::is_valid_variable_name;
    let mut seen = std::collections::HashSet::new();
    variables.iter().all(|(name, value)| {
        let name = name.trim();
        is_valid_variable_name(name) && seen.insert(name.to_lowercase()) && !value.trim().is_empty()
    })
}

/// Whether `sources` can be kept: every row passes the checks of the C# settings page and
/// import (`SettingsViewModel.SharedSessionSources.cs:247`).
fn shared_sources_accepted(sources: &[heimdall_core::shared_sessions::SharedSource]) -> bool {
    heimdall_core::shared_sessions::source_problems(sources)
        .iter()
        .all(|problems| problems.is_ok())
}
