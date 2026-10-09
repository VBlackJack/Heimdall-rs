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

//! The settings of Heimdall-rs, beside its profiles: what the Settings page changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::credential_provider::{MAX_TIMEOUT, MIN_TIMEOUT, ProviderKind, ProviderSettings};
use crate::lockout::{LOCKOUT_DURATION, Lockout};
use crate::pin::PinHash;
use crate::profile::{
    ProfileId, RESOLUTION_PRESETS, RdpDefaults, SshMode, preset_fits, resolution_preset,
    resolution_text,
};
use crate::store::{StoreError, write_atomic};

/// Name of the settings file, beside the profile file.
pub const SETTINGS_FILE_NAME: &str = "settings.toml";

/// Format version written into the settings file.
pub const SETTINGS_FILE_VERSION: u32 = 1;

/// The terminal's colours, as the C# Heimdall names its schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// The terminal's own: black, white text, the Tango colours.
    Standard,
    /// Dracula, the Heimdall design system's.
    #[default]
    Dracula,
    /// Solarized Dark.
    SolarizedDark,
    /// Monokai.
    Monokai,
    /// Nord.
    Nord,
}

impl ColorScheme {
    /// Every scheme, in the order the C# Settings page lists them.
    pub const ALL: [Self; 5] = [
        Self::Standard,
        Self::Dracula,
        Self::SolarizedDark,
        Self::Monokai,
        Self::Nord,
    ];

    /// The name the file holds, the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Default",
            Self::Dracula => "Dracula",
            Self::SolarizedDark => "Solarized Dark",
            Self::Monokai => "Monokai",
            Self::Nord => "Nord",
        }
    }

    /// The scheme named `name`, whatever its case, as the C# Heimdall reads it; Dracula
    /// for a name it does not know.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// Which terminals broadcast input reaches besides the one typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BroadcastScope {
    /// The panes of the tab typed into, as the C#'s default: it cannot reach a terminal not
    /// in sight.
    #[default]
    CurrentTab,
    /// Every terminal of every tab, asked before it starts.
    AllTabs,
    /// The tabs marked as targets.
    SelectedTabs,
}

impl BroadcastScope {
    /// The name the file holds.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CurrentTab => "CurrentTab",
            Self::AllTabs => "AllTabs",
            Self::SelectedTabs => "SelectedTabs",
        }
    }

    /// The scope named `name`, the C#'s `SelectedPanes` being the tabs marked; for a name
    /// not known, the current tab, the narrowest.
    #[must_use]
    pub fn named(name: &str) -> Self {
        let name = name.trim();
        if name.eq_ignore_ascii_case(Self::AllTabs.name()) {
            Self::AllTabs
        } else if name.eq_ignore_ascii_case(Self::SelectedTabs.name())
            || name.eq_ignore_ascii_case(SELECTED_PANES)
        {
            Self::SelectedTabs
        } else {
            Self::CurrentTab
        }
    }
}

/// The C#'s name for the tabs marked, read as [`BroadcastScope::SelectedTabs`].
const SELECTED_PANES: &str = "SelectedPanes";

/// The window's theme, as the C# `DefaultTheme` names the `ThemeForge` palettes. Apart
/// from the terminals' colour scheme, as in the C#.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppTheme {
    /// The canonical Dracula palette.
    Dracula,
    /// Dracula with its comment colour lifted to be readable: the C# default.
    #[default]
    Drakul,
    /// Warm umber.
    Striga,
    /// Warm amber.
    Cinder,
    /// Fern green.
    Bracken,
    /// Petrol teal.
    Tarn,
    /// Slate cyan.
    Mortis,
    /// Slate blue.
    Slate,
    /// Navy with an indigo accent.
    Magellan,
    /// Deep indigo.
    Voivode,
    /// Velvet violet.
    Carmilla,
    /// Slate mauve.
    Whitby,
    /// Rose.
    Vesper,
    /// Warm cream, light.
    Parchment,
    /// Cool ivory, light.
    Folio,
    /// Green moved to viridian.
    Wormwood,
    /// Orange moved to amber.
    Sconce,
    /// White on black, yellow and cyan accents, every control outlined: the Windows "High
    /// Contrast Black" scheme. Taken by itself while Windows high contrast is on.
    HighContrast,
}

impl AppTheme {
    /// Every theme, in the order of the C# list, high contrast last.
    pub const ALL: [Self; 18] = [
        Self::Dracula,
        Self::Drakul,
        Self::Striga,
        Self::Cinder,
        Self::Bracken,
        Self::Tarn,
        Self::Mortis,
        Self::Slate,
        Self::Magellan,
        Self::Voivode,
        Self::Carmilla,
        Self::Whitby,
        Self::Vesper,
        Self::Parchment,
        Self::Folio,
        Self::Wormwood,
        Self::Sconce,
        Self::HighContrast,
    ];

    /// The name the file holds: the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dracula => "Dracula",
            Self::Drakul => "Drakul",
            Self::Striga => "Striga",
            Self::Cinder => "Cinder",
            Self::Bracken => "Bracken",
            Self::Tarn => "Tarn",
            Self::Mortis => "Mortis",
            Self::Slate => "Slate",
            Self::Magellan => "Magellan",
            Self::Voivode => "Voivode",
            Self::Carmilla => "Carmilla",
            Self::Whitby => "Whitby",
            Self::Vesper => "Vesper",
            Self::Parchment => "Parchment",
            Self::Folio => "Folio",
            Self::Wormwood => "Wormwood",
            Self::Sconce => "Sconce",
            Self::HighContrast => "HighContrast",
        }
    }

    /// The theme named `name`, whatever its case; the default for a name not known, as the
    /// C# load.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// The colour the window's accent takes, as the C# `AccentTint`: the theme's own, or one of
/// its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Accent {
    /// The theme's own accent.
    #[default]
    Default,
    /// The theme's blue.
    Blue,
    /// The theme's cyan.
    Cyan,
    /// The theme's green.
    Green,
    /// The theme's orange.
    Orange,
    /// The theme's pink.
    Pink,
    /// The theme's purple.
    Purple,
    /// The theme's red.
    Red,
    /// The theme's yellow.
    Yellow,
}

impl Accent {
    /// Every accent, in the order of the C# list.
    pub const ALL: [Self; 9] = [
        Self::Default,
        Self::Blue,
        Self::Cyan,
        Self::Green,
        Self::Orange,
        Self::Pink,
        Self::Purple,
        Self::Red,
        Self::Yellow,
    ];

    /// The name the file holds: the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Blue => "Blue",
            Self::Cyan => "Cyan",
            Self::Green => "Green",
            Self::Orange => "Orange",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
            Self::Red => "Red",
            Self::Yellow => "Yellow",
        }
    }

    /// The accent named `name`, whatever its case; the default for a name not known, as the
    /// C# load.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|accent| accent.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// Which SSH agent's keys are offered first, or alone, as the C# `SshAgentPreference`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentPreference {
    /// Every agent running, the Windows OpenSSH agent's keys first.
    #[default]
    OpenSshFirst,
    /// Every agent running, Pageant's keys first.
    PageantFirst,
    /// The Windows OpenSSH agent alone.
    OpenSshOnly,
    /// Pageant alone.
    PageantOnly,
}

impl AgentPreference {
    /// Every preference, in the order of the C# list.
    pub const ALL: [Self; 4] = [
        Self::OpenSshFirst,
        Self::PageantFirst,
        Self::OpenSshOnly,
        Self::PageantOnly,
    ];

    /// The name the file holds: the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenSshFirst => "AutoOpenSshFirst",
            Self::PageantFirst => "AutoPageantFirst",
            Self::OpenSshOnly => "OpenSshOnly",
            Self::PageantOnly => "PageantOnly",
        }
    }

    /// The preference named `name`; the default for a name not known, as the C# load.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|preference| preference.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// The execution policy a local `PowerShell` is started with, as the C#
/// `PowerShellExecutionPolicy`: its own unless another is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionPolicy {
    /// `PowerShell`'s own: nothing is passed.
    #[default]
    Default,
    /// `-ExecutionPolicy Bypass`.
    Bypass,
    /// `-ExecutionPolicy RemoteSigned`.
    RemoteSigned,
    /// `-ExecutionPolicy Unrestricted`.
    Unrestricted,
    /// `-ExecutionPolicy AllSigned`.
    AllSigned,
}

impl ExecutionPolicy {
    /// Every policy, in the order of the C# list.
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Bypass,
        Self::RemoteSigned,
        Self::Unrestricted,
        Self::AllSigned,
    ];

    /// The name the file holds and `PowerShell` takes: the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Bypass => "Bypass",
            Self::RemoteSigned => "RemoteSigned",
            Self::Unrestricted => "Unrestricted",
            Self::AllSigned => "AllSigned",
        }
    }

    /// The policy named `name`; the default for a name not known, as the C# load.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|policy| policy.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// What Ctrl+V does in a terminal. The C# pastes; vim and readline take ^V as "the next
/// key as it is", which full-screen programs use most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtrlVPaste {
    /// Ctrl+V pastes, as the C# and Windows Terminal.
    Always,
    /// Ctrl+V pastes, except while a full-screen program (the alternate screen) is shown,
    /// which gets ^V.
    #[default]
    OutsideFullScreenPrograms,
    /// Ctrl+V is ^V for the session; Ctrl+Shift+V pastes.
    Never,
}

impl CtrlVPaste {
    /// Every choice, in the order the list shows them.
    pub const ALL: [Self; 3] = [Self::Always, Self::OutsideFullScreenPrograms, Self::Never];

    /// Its name in the settings file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OutsideFullScreenPrograms => "outside-full-screen-programs",
            Self::Never => "never",
        }
    }

    /// The choice named `name`; the default for a name not known.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|choice| choice.name() == name.trim())
            .unwrap_or_default()
    }

    /// Whether Ctrl+V pastes, the alternate screen shown or not.
    #[must_use]
    pub const fn pastes(self, alternate_screen: bool) -> bool {
        match self {
            Self::Always => true,
            Self::OutsideFullScreenPrograms => !alternate_screen,
            Self::Never => false,
        }
    }
}

/// What Ctrl+K does in a terminal. The C# opens Quick Connect, wherever the keyboard is;
/// readline and emacs take ^K as "delete to the end of the line".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtrlKTerminal {
    /// Ctrl+K opens Quick Connect, as the C#.
    #[default]
    QuickConnect,
    /// Ctrl+K is ^K for the session; Ctrl+Shift+K opens Quick Connect.
    SendToSession,
}

impl CtrlKTerminal {
    /// Every choice, in the order the list shows them.
    pub const ALL: [Self; 2] = [Self::QuickConnect, Self::SendToSession];

    /// Its name in the settings file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::QuickConnect => "quick-connect",
            Self::SendToSession => "send-to-session",
        }
    }

    /// The choice named `name`; the default for a name not known.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|choice| choice.name() == name.trim())
            .unwrap_or_default()
    }

    /// Whether Ctrl+K in a terminal opens Quick Connect rather than reaching the session.
    #[must_use]
    pub const fn opens_quick_connect(self) -> bool {
        matches!(self, Self::QuickConnect)
    }
}

/// Where transcripts go when no folder is chosen, beside the settings, as the C# one.
pub const DEFAULT_SESSION_LOG_DIRECTORY: &str = "logs/sessions";

/// Days transcripts are kept by default: all of them, as the C# default. They can be audit
/// records, so deleting them is the user's choice to make.
pub const SESSION_LOG_RETENTION_DAYS_DEFAULT: u32 = 0;

/// Fewest days of transcript retention accepted besides 0, as the C# setting's range.
pub const SESSION_LOG_RETENTION_DAYS_MIN: u32 = 7;

/// Most days of transcript retention accepted, as the C# setting's range.
pub const SESSION_LOG_RETENTION_DAYS_MAX: u32 = 3650;

/// Whether `days` is a transcript retention the settings accept: 0 to keep every
/// transcript, or within the C# range.
#[must_use]
pub fn session_log_retention_days_accepted(days: u32) -> bool {
    days == 0 || (SESSION_LOG_RETENTION_DAYS_MIN..=SESSION_LOG_RETENTION_DAYS_MAX).contains(&days)
}

/// What the Settings page changes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent on/off settings, one per C# checkbox"
)]
pub struct Settings {
    /// The terminal's colours.
    pub color_scheme: ColorScheme,
    /// Which terminals broadcast input reaches.
    pub broadcast_scope: BroadcastScope,
    /// Every SSH, Telnet and local session keeps a transcript from when it connects.
    pub session_logging: bool,
    /// Where transcripts go: a folder, or one relative to the settings file's.
    pub session_log_directory: String,
    /// Days a transcript is kept after its last write, as the C# `SessionLogRetentionDays`:
    /// 0 keeps every one, else within [`SESSION_LOG_RETENTION_DAYS_MIN`] and
    /// [`SESSION_LOG_RETENTION_DAYS_MAX`].
    pub session_log_retention_days: u32,
    /// Size of the terminals' text a new tab starts at, and Ctrl+0 comes back to, within
    /// [`TERMINAL_FONT_SIZE_MIN`] and [`TERMINAL_FONT_SIZE_MAX`].
    pub terminal_font_size: u16,
    /// The family of the terminals' text, as the C# `TerminalFontFamily`: by name, never
    /// empty; [`TERMINAL_FONT_FAMILY_DEFAULT`] unless chosen. One the computer lacks is
    /// drawn in the default, said where it is chosen.
    pub terminal_font_family: String,
    /// The language chosen; `None` follows the desktop's.
    pub language: Option<Language>,
    /// The window's theme, as the C# `DefaultTheme`: Drakul unless chosen.
    pub theme: AppTheme,
    /// The window's accent, as the C# `AccentTint`: the theme's own unless chosen.
    pub accent: Accent,
    /// Wrong master passwords in a row when the application starts, kept across runs as
    /// the C# startup gate keeps them: quitting does not give the tries back.
    pub vault_unlock: Lockout,
    /// When the master password last opened the vault, on this computer, as the C#
    /// `VaultLastMasterUnlockUtc`: a Windows Hello unlock does not change it. It says when
    /// Windows Hello must give way to the master password again
    /// ([`WindowsHello::vault_max_days`]).
    pub vault_last_master_unlock: Option<SystemTime>,
    /// Minutes without any input on the computer after which the workspace locks, as the C#
    /// `AutoLockIdleMinutes`: [`AUTO_LOCK_IDLE_MINUTES_OFF`] never locks, else at most
    /// [`AUTO_LOCK_IDLE_MINUTES_MAX`]. Only with a master password set.
    pub auto_lock_idle_minutes: u32,
    /// Locking the workspace also closes every session, as the C# `DisconnectOnLock`: off,
    /// the sessions go on running hidden behind the lock.
    pub disconnect_on_lock: bool,
    /// The application PIN asked at start, as the C# one; `None` when none is set.
    pub pin: Option<PinHash>,
    /// Wrong PINs in a row, kept across runs as the master password's are.
    pub pin_unlock: Lockout,
    /// The external credential provider.
    pub credential_provider: ProviderSettings,
    /// An SSH session whose connection is lost opens again by itself, as the C#
    /// `SshAutoReconnect`: off unless chosen.
    pub ssh_auto_reconnect: bool,
    /// Attempts before the reconnect is left to the user, within
    /// [`SSH_AUTO_RECONNECT_ATTEMPTS_MIN`] and [`SSH_AUTO_RECONNECT_ATTEMPTS_MAX`].
    pub ssh_auto_reconnect_attempts: u32,
    /// Attempts of an RDP desktop's auto-reconnect, as the C# `RdpAutoReconnectMaxAttempts`:
    /// within [`RDP_AUTO_RECONNECT_ATTEMPTS_MIN`] and [`RDP_AUTO_RECONNECT_ATTEMPTS_MAX`].
    pub rdp_auto_reconnect_attempts: u32,
    /// Seconds an RDP connection may take to log on before it is given up, 0 for no limit,
    /// as the C# `RdpConnectWatchdogTimeoutMs`.
    pub rdp_connect_timeout: u32,
    /// The sizes RDP tabs' Resolution menus offer, as the C# `RdpResolutionPresets`; empty
    /// offers [`RESOLUTION_PRESETS`]. See [`Settings::resolution_presets`].
    pub rdp_resolution_presets: Vec<(u16, u16)>,
    /// Seconds between two anti-idle keys of an RDP session whose profile asks for them, as
    /// the C# `AntiIdleIntervalSeconds`: 0 turns them off, else within
    /// [`ANTI_IDLE_INTERVAL_MIN`] and [`ANTI_IDLE_INTERVAL_MAX`].
    pub anti_idle_interval: u32,
    /// Seconds between two SSH keep-alives of every connection (shells, files, tunnels,
    /// gateways), as the C# `SshKeepAliveIntervalSeconds`: within
    /// [`SSH_KEEP_ALIVE_INTERVAL_MIN`] and [`SSH_KEEP_ALIVE_INTERVAL_MAX`].
    pub ssh_keep_alive_interval: u32,
    /// Seconds of no input after which an SSH shell is sent a bare carriage return, so a
    /// remote `TMOUT` does not log the user out, as the C# `SshTmoutResetIntervalSeconds`: 0
    /// turns it off, at most [`SSH_TMOUT_RESET_INTERVAL_MAX`].
    pub ssh_tmout_reset_interval: u32,
    /// The RDP options profiles following the application's take.
    pub rdp_defaults: RdpDefaults,
    /// The program a server's file is edited with, as the C# `ExternalEditorPath`; empty
    /// takes the system's own text editor.
    pub external_editor: String,
    /// `PuTTY`, which an SSH profile in the external mode opens in, as the C# `PuttyPath`;
    /// empty looks for it in the folders of `PATH`.
    pub putty_path: String,
    /// Where the shell of a new SSH profile opens, as the C# `SshDefaultMode`: in a tab
    /// unless chosen. Only the profile form starts with it, as the C# "Add server".
    pub ssh_default_mode: SshMode,
    /// The X server started for X11 forwarding when none runs, as the C# `X11ServerPath`;
    /// empty tries the places the C# knows, then the folders of `PATH`.
    pub x11_server_path: String,
    /// An X server is started when X11 forwarding needs one and none runs, as the C#
    /// `X11AutoStart`: on.
    pub x11_auto_start: bool,
    /// Which SSH agent's keys are offered first, or alone; applied to the next connection.
    pub ssh_agent_preference: AgentPreference,
    /// The execution policy a local `PowerShell` is started with.
    pub powershell_execution_policy: ExecutionPolicy,
    /// What Ctrl+V does in a terminal.
    pub ctrl_v_paste: CtrlVPaste,
    /// What Ctrl+K does in a terminal.
    pub ctrl_k_terminal: CtrlKTerminal,
    /// The tunnels panel starts collapsed, as the C# `CollapseTunnelsPanelByDefault`: on.
    pub collapse_tunnels_panel: bool,
    /// The computer kept from sleeping while a session is open, as the C#
    /// `PreventSleepDuringSession`: on.
    pub prevent_sleep: bool,
    /// Most sessions open at once, as the C# `MaxEmbeddedSessions`; 0 for no limit, the
    /// default here: a session costs no embedded control as the C# one does.
    pub max_sessions: u32,
    /// The application writes its diagnostics log, as the C# `EnableLogging`: on.
    pub diagnostics_log: bool,
    /// Whether, and how often, every server is checked for an answer in the background.
    pub reachability: Reachability,
    /// The SFTP browser, the pane it docks beside an SSH shell, and the file browser docked
    /// beside a local shell.
    pub sftp_browser: SftpBrowser,
    /// The tree's rows show the gateway a profile goes through, as the C#
    /// `ShowGatewayBadge`: on. A view choice of the tree's filter menu, kept across runs.
    pub show_gateway_badge: bool,
    /// Whether, and how often, a newer release is looked for.
    pub updates: Updates,
    /// The last look for a newer release and the release skipped, kept across runs on
    /// this computer as the C# `UpdateLastCheckUtc` and `UpdateSkippedVersion`.
    pub update_check: UpdateCheck,
    /// Windows Hello asked before a connection uses what is saved for it.
    pub windows_hello: WindowsHello,
    /// The SSH gateway of the last profile saved from the profile form, as the C#
    /// `LastUsedGatewayId` (`AppSettings.cs:363`): a new profile's form starts on it while it
    /// still exists. This computer's own, as the C# settings transfer leaves it behind: never
    /// exported, and kept by a reset.
    pub last_used_gateway: Option<ProfileId>,
}

/// The SFTP browser's settings, as the C# `SftpBrowserEnabled`, `SftpAutoOpenOnSsh` and
/// `SftpFollowSshDirectory`: the last two only apply while the first is on. With them, the
/// file browser docked beside a local shell, which the C# always docks, and whether it
/// follows the shell's working folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent on/off settings, one per checkbox of the card"
)]
pub struct SftpBrowser {
    /// A server's files can be browsed over SFTP, as the C# `SftpBrowserEnabled`: on. Off,
    /// no SFTP Files tab opens.
    pub enabled: bool,
    /// An SSH shell, once connected, gets its server's files in a pane beside it, as the C#
    /// `SftpAutoOpenOnSsh`: on.
    pub auto_open_on_ssh: bool,
    /// The SFTP pane beside a shell follows the working folder the shell reports (OSC 7),
    /// as the C# `SftpFollowSshDirectory`: off. It seeds each SFTP pane's "cwd" toggle when
    /// the pane opens; the toggle then acts on its pane alone.
    pub follow_ssh_directory: bool,
    /// A local shell, once started, gets this computer's files in a browser beside it: on,
    /// as the C# always docks it. Apart from the SFTP browser: no server is involved.
    pub dock_local_browser: bool,
    /// The file browser docked beside a local shell follows the working folder the shell
    /// reports (OSC 7, or OSC 9;9 on Windows): on. The C# makes the shell follow the
    /// browser instead, typing `cd` into it; nothing is typed into a shell here, and the
    /// browser starts in the shell's folder, so following it keeps the two together. A
    /// shell that never reports moves nothing. It seeds each local browser's "cwd" toggle
    /// when the browser docks.
    pub follow_local_directory: bool,
}

impl SftpBrowser {
    /// Whether an SSH shell connected opens its files beside it: the browser on, and the
    /// auto-open on.
    #[must_use]
    pub const fn auto_opens(self) -> bool {
        self.enabled && self.auto_open_on_ssh
    }
}

impl Default for SftpBrowser {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_open_on_ssh: true,
            follow_ssh_directory: false,
            dock_local_browser: true,
            follow_local_directory: true,
        }
    }
}

/// The background check of every server's address, as the C# session health monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reachability {
    /// It runs, as the C# `SessionHealthMonitorEnabled`: on.
    pub enabled: bool,
    /// Seconds between two checks, within [`REACHABILITY_INTERVAL_MIN`] and
    /// [`REACHABILITY_INTERVAL_MAX`].
    pub interval: u32,
    /// Milliseconds a server has to answer, within [`REACHABILITY_TIMEOUT_MIN`] and
    /// [`REACHABILITY_TIMEOUT_MAX`].
    pub timeout: u32,
    /// Servers checked at once, within [`REACHABILITY_PROBES_MIN`] and
    /// [`REACHABILITY_PROBES_MAX`].
    pub probes: u32,
}

impl Default for Reachability {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: REACHABILITY_INTERVAL_DEFAULT,
            timeout: REACHABILITY_TIMEOUT_DEFAULT,
            probes: REACHABILITY_PROBES_DEFAULT,
        }
    }
}

/// Seconds between two reachability checks by default, as the C#
/// `SessionHealthCheckIntervalSeconds`.
pub const REACHABILITY_INTERVAL_DEFAULT: u32 = 60;

/// Shortest reachability interval accepted, in seconds, as the C# setting's range.
pub const REACHABILITY_INTERVAL_MIN: u32 = 15;

/// Longest reachability interval accepted, in seconds, as the C# setting's range.
pub const REACHABILITY_INTERVAL_MAX: u32 = 3600;

/// Whether `seconds` is a reachability interval the settings accept.
#[must_use]
pub fn reachability_interval_accepted(seconds: u32) -> bool {
    (REACHABILITY_INTERVAL_MIN..=REACHABILITY_INTERVAL_MAX).contains(&seconds)
}

/// Milliseconds a server has to answer a reachability check by default, as the C#
/// `SessionHealthProbeTimeoutMs`.
pub const REACHABILITY_TIMEOUT_DEFAULT: u32 = 2000;

/// Shortest reachability timeout accepted, in milliseconds, as the C# setting's range.
pub const REACHABILITY_TIMEOUT_MIN: u32 = 250;

/// Longest reachability timeout accepted, in milliseconds, as the C# setting's range.
pub const REACHABILITY_TIMEOUT_MAX: u32 = 30_000;

/// Whether `millis` is a reachability timeout the settings accept.
#[must_use]
pub fn reachability_timeout_accepted(millis: u32) -> bool {
    (REACHABILITY_TIMEOUT_MIN..=REACHABILITY_TIMEOUT_MAX).contains(&millis)
}

/// Servers checked at once by default, as the C# `SessionHealthMaxConcurrent`.
pub const REACHABILITY_PROBES_DEFAULT: u32 = 10;

/// Fewest servers checked at once accepted, as the C# setting's range.
pub const REACHABILITY_PROBES_MIN: u32 = 1;

/// Most servers checked at once accepted, as the C# setting's range.
pub const REACHABILITY_PROBES_MAX: u32 = 50;

/// Whether `count` is a number of servers checked at once the settings accept.
#[must_use]
pub fn reachability_probes_accepted(count: u32) -> bool {
    (REACHABILITY_PROBES_MIN..=REACHABILITY_PROBES_MAX).contains(&count)
}

/// The look for a newer release, as the C# `UpdateCheckEnabled` and
/// `UpdateCheckIntervalHours`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Updates {
    /// A newer release is looked for by itself: on, as the C#.
    pub enabled: bool,
    /// Hours between two looks, within [`UPDATE_INTERVAL_HOURS_MIN`] and
    /// [`UPDATE_INTERVAL_HOURS_MAX`].
    pub interval_hours: u32,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: UPDATE_INTERVAL_HOURS_DEFAULT,
        }
    }
}

/// Hours between two looks for a newer release by default, as the C#.
pub const UPDATE_INTERVAL_HOURS_DEFAULT: u32 = 24;

/// Shortest interval accepted, in hours, as the C# setting's range.
pub const UPDATE_INTERVAL_HOURS_MIN: u32 = 1;

/// Longest interval accepted, in hours: a year, as the C# setting's range.
pub const UPDATE_INTERVAL_HOURS_MAX: u32 = 8760;

/// Whether `hours` is an update interval the settings accept.
#[must_use]
pub fn update_interval_accepted(hours: u32) -> bool {
    (UPDATE_INTERVAL_HOURS_MIN..=UPDATE_INTERVAL_HOURS_MAX).contains(&hours)
}

/// What this computer remembers of the looks for a newer release: not a preference, so an
/// export leaves it behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateCheck {
    /// When the last look got an answer; `None` before the first one.
    pub last_check: Option<SystemTime>,
    /// The release tag the user chose to skip, as the release names it: `v2026.100901`.
    pub skipped: Option<String>,
}

/// Windows Hello asked before connecting, as the C# `RequireWindowsHelloOnConnect` and
/// `WindowsHelloGraceMinutes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsHello {
    /// A connection waits for a successful Windows Hello verification: off, as the C#.
    /// Where Windows Hello is unavailable, connections are refused while it is on.
    pub require_on_connect: bool,
    /// Minutes a successful verification still counts, within
    /// [`WINDOWS_HELLO_GRACE_MINUTES_MAX`]; [`WINDOWS_HELLO_GRACE_MINUTES_NONE`] asks every
    /// time. The verification itself is kept in memory only.
    pub grace_minutes: u32,
    /// Days Windows Hello unlocks the vault since the master password last did, as the C#
    /// `VaultHelloMaxDaysBeforeMasterPassword`: then the master password is asked once.
    /// [`WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER`] never asks, as the C# default; at most
    /// [`WINDOWS_HELLO_VAULT_MAX_DAYS_MAX`].
    pub vault_max_days: u32,
}

impl Default for WindowsHello {
    fn default() -> Self {
        Self {
            require_on_connect: false,
            grace_minutes: WINDOWS_HELLO_GRACE_MINUTES_DEFAULT,
            vault_max_days: WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER,
        }
    }
}

/// Minutes a Windows Hello verification counts by default, as the C#
/// `DefaultWindowsHelloGraceMinutes`.
pub const WINDOWS_HELLO_GRACE_MINUTES_DEFAULT: u32 = 5;

/// The Windows Hello grace that asks every time, as the C# `0 = always re-verify`.
pub const WINDOWS_HELLO_GRACE_MINUTES_NONE: u32 = 0;

/// Longest grace accepted, in minutes, as the C# setting's range: a day.
pub const WINDOWS_HELLO_GRACE_MINUTES_MAX: u32 = 1440;

/// Whether `minutes` is a Windows Hello grace the settings accept: none, which asks every
/// time, up to [`WINDOWS_HELLO_GRACE_MINUTES_MAX`].
#[must_use]
pub fn windows_hello_grace_minutes_accepted(minutes: u32) -> bool {
    minutes <= WINDOWS_HELLO_GRACE_MINUTES_MAX
}

/// The days Windows Hello unlocks the vault without the master password ever being asked
/// again, as the C# `0 disables the periodic re-authentication policy`; the default.
pub const WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER: u32 = 0;

/// Most days accepted before the master password is asked again, as the C# setting's
/// range: ten years.
pub const WINDOWS_HELLO_VAULT_MAX_DAYS_MAX: u32 = 3650;

/// Whether `days` is a number of days before the master password is asked again that the
/// settings accept: never, up to [`WINDOWS_HELLO_VAULT_MAX_DAYS_MAX`].
#[must_use]
pub fn windows_hello_vault_max_days_accepted(days: u32) -> bool {
    days <= WINDOWS_HELLO_VAULT_MAX_DAYS_MAX
}

/// A language the application is written in, as the C# language list offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// English, the language every text is written in first.
    English,
    /// French.
    French,
    /// Spanish.
    Spanish,
}

impl Language {
    /// Every language, in the order of the C# list.
    pub const ALL: [Self; 3] = [Self::English, Self::French, Self::Spanish];

    /// Its code, as the C# `DefaultLocale` and the translation folders name it.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::French => "fr",
            Self::Spanish => "es",
        }
    }

    /// The language of `code`, whatever its case; `None` for one not offered.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code().eq_ignore_ascii_case(code.trim()))
    }
}

/// Size of the terminals' text unless chosen: the one this terminal was drawn at before.
pub const TERMINAL_FONT_SIZE_DEFAULT: u16 = 15;
/// Smallest terminal font size accepted, as the C# setting's range.
pub const TERMINAL_FONT_SIZE_MIN: u16 = 8;

/// Family of the terminals' text unless chosen: the one embedded in the application, which
/// every terminal was drawn in before the family could be chosen.
pub const TERMINAL_FONT_FAMILY_DEFAULT: &str = "Source Code Pro";

/// `family` as the settings keep it: trimmed, the default when nothing is left.
#[must_use]
pub fn terminal_font_family(family: &str) -> String {
    match family.trim() {
        "" => TERMINAL_FONT_FAMILY_DEFAULT.to_owned(),
        named => named.to_owned(),
    }
}
/// Attempts of an SSH auto-reconnect by default, as the C# `SshAutoReconnectAttempts`.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT: u32 = 3;

/// Fewest attempts of an SSH auto-reconnect accepted, as the C# setting's range.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_MIN: u32 = 1;

/// Most attempts of an SSH auto-reconnect accepted, as the C# setting's range.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_MAX: u32 = 10;

/// Most attempts of an RDP auto-reconnect, and their number by default, as the C#
/// `DefaultRdpAutoReconnectMaxAttempts`.
pub const RDP_AUTO_RECONNECT_ATTEMPTS_MAX: u32 = 20;

/// Fewest attempts of an RDP auto-reconnect accepted, as the C# setting's range.
pub const RDP_AUTO_RECONNECT_ATTEMPTS_MIN: u32 = 1;

/// Seconds an RDP connection may take by default, as the C# watchdog's 45 000 ms.
pub const RDP_CONNECT_TIMEOUT_DEFAULT: u32 = 45;
/// Fewest seconds accepted besides 0, as the C# range's 5 000 ms.
pub const RDP_CONNECT_TIMEOUT_MIN: u32 = 5;
/// Most seconds accepted, as the C# range's 600 000 ms.
pub const RDP_CONNECT_TIMEOUT_MAX: u32 = 600;

/// Whether `seconds` is an RDP connection timeout the settings accept: 0 for none, or within
/// the C# range.
#[must_use]
pub fn rdp_connect_timeout_accepted(seconds: u32) -> bool {
    seconds == 0 || (RDP_CONNECT_TIMEOUT_MIN..=RDP_CONNECT_TIMEOUT_MAX).contains(&seconds)
}

/// Whether `attempts` is a number of RDP auto-reconnect attempts the settings accept.
#[must_use]
pub fn rdp_auto_reconnect_attempts_accepted(attempts: u32) -> bool {
    (RDP_AUTO_RECONNECT_ATTEMPTS_MIN..=RDP_AUTO_RECONNECT_ATTEMPTS_MAX).contains(&attempts)
}

/// `value` read from the file when `accepted`; `default` when absent or out of the range,
/// as the C# load warns and keeps the default.
/// Sessions open at once by default: no limit.
pub const MAX_SESSIONS_DEFAULT: u32 = 0;
/// Most sessions a limit may allow, as the C# setting's range.
pub const MAX_SESSIONS_MAX: u32 = 20;

/// Whether `max` is a limit of sessions the settings accept: 0 for none, or within the C#
/// range.
#[must_use]
pub fn max_sessions_accepted(max: u32) -> bool {
    max <= MAX_SESSIONS_MAX
}

fn within(value: Option<u32>, accepted: fn(u32) -> bool, default: u32) -> u32 {
    value.filter(|value| accepted(*value)).unwrap_or(default)
}

/// Whether `attempts` is a number of SSH auto-reconnect attempts the settings accept.
#[must_use]
pub fn ssh_auto_reconnect_attempts_accepted(attempts: u32) -> bool {
    (SSH_AUTO_RECONNECT_ATTEMPTS_MIN..=SSH_AUTO_RECONNECT_ATTEMPTS_MAX).contains(&attempts)
}

/// Seconds between two anti-idle keys by default, as the C# `AntiIdleIntervalSeconds`.
pub const ANTI_IDLE_INTERVAL_DEFAULT: u32 = 60;

/// Shortest anti-idle interval accepted, in seconds, as the C# setting's range.
pub const ANTI_IDLE_INTERVAL_MIN: u32 = 10;

/// Longest anti-idle interval accepted, in seconds, as the C# setting's range.
pub const ANTI_IDLE_INTERVAL_MAX: u32 = 3600;

/// Whether `seconds` is an anti-idle interval the settings accept: 0, which turns it off, or
/// one within the C# range.
#[must_use]
pub fn anti_idle_interval_accepted(seconds: u32) -> bool {
    seconds == 0 || (ANTI_IDLE_INTERVAL_MIN..=ANTI_IDLE_INTERVAL_MAX).contains(&seconds)
}

/// Seconds between two SSH keep-alives by default, as the C#
/// `DefaultSshKeepAliveIntervalSeconds`.
pub const SSH_KEEP_ALIVE_INTERVAL_DEFAULT: u32 = 30;

/// Shortest SSH keep-alive interval accepted, in seconds, as the C# setting's range.
pub const SSH_KEEP_ALIVE_INTERVAL_MIN: u32 = 5;

/// Longest SSH keep-alive interval accepted, in seconds, as the C# setting's range.
pub const SSH_KEEP_ALIVE_INTERVAL_MAX: u32 = 600;

/// Whether `seconds` is an SSH keep-alive interval the settings accept.
#[must_use]
pub fn ssh_keep_alive_interval_accepted(seconds: u32) -> bool {
    (SSH_KEEP_ALIVE_INTERVAL_MIN..=SSH_KEEP_ALIVE_INTERVAL_MAX).contains(&seconds)
}

/// Seconds of no input before a TMOUT reset by default, as the C#
/// `DefaultSshTmoutResetIntervalSeconds`.
pub const SSH_TMOUT_RESET_INTERVAL_DEFAULT: u32 = 240;

/// Longest TMOUT reset interval accepted, in seconds, as the C# setting's range.
pub const SSH_TMOUT_RESET_INTERVAL_MAX: u32 = 3600;

/// Whether `seconds` is a TMOUT reset interval the settings accept: 0, which turns it off,
/// up to [`SSH_TMOUT_RESET_INTERVAL_MAX`].
#[must_use]
pub fn ssh_tmout_reset_interval_accepted(seconds: u32) -> bool {
    seconds <= SSH_TMOUT_RESET_INTERVAL_MAX
}

/// The idle auto-lock threshold that turns it off, and its default, as the C#.
pub const AUTO_LOCK_IDLE_MINUTES_OFF: u32 = 0;

/// Idle auto-lock threshold unless chosen: off.
pub const AUTO_LOCK_IDLE_MINUTES_DEFAULT: u32 = AUTO_LOCK_IDLE_MINUTES_OFF;

/// Longest idle auto-lock threshold accepted, in minutes, as the C# setting's range: a day.
pub const AUTO_LOCK_IDLE_MINUTES_MAX: u32 = 1440;

/// Whether `minutes` is an idle auto-lock threshold the settings accept: off, up to
/// [`AUTO_LOCK_IDLE_MINUTES_MAX`].
#[must_use]
pub fn auto_lock_idle_minutes_accepted(minutes: u32) -> bool {
    minutes <= AUTO_LOCK_IDLE_MINUTES_MAX
}

/// Largest terminal font size accepted, as the C# setting's range.
pub const TERMINAL_FONT_SIZE_MAX: u16 = 72;

/// Whether `size` is a terminal font size the settings accept.
#[must_use]
pub fn terminal_font_size_accepted(size: u16) -> bool {
    (TERMINAL_FONT_SIZE_MIN..=TERMINAL_FONT_SIZE_MAX).contains(&size)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            color_scheme: ColorScheme::default(),
            broadcast_scope: BroadcastScope::default(),
            session_logging: false,
            session_log_directory: DEFAULT_SESSION_LOG_DIRECTORY.to_owned(),
            session_log_retention_days: SESSION_LOG_RETENTION_DAYS_DEFAULT,
            terminal_font_size: TERMINAL_FONT_SIZE_DEFAULT,
            terminal_font_family: TERMINAL_FONT_FAMILY_DEFAULT.to_owned(),
            language: None,
            theme: AppTheme::default(),
            accent: Accent::default(),
            vault_unlock: Lockout::default(),
            vault_last_master_unlock: None,
            auto_lock_idle_minutes: AUTO_LOCK_IDLE_MINUTES_DEFAULT,
            disconnect_on_lock: false,
            pin: None,
            pin_unlock: Lockout::default(),
            credential_provider: ProviderSettings::default(),
            ssh_auto_reconnect: false,
            ssh_auto_reconnect_attempts: SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
            rdp_auto_reconnect_attempts: RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
            rdp_connect_timeout: RDP_CONNECT_TIMEOUT_DEFAULT,
            rdp_resolution_presets: RESOLUTION_PRESETS.to_vec(),
            anti_idle_interval: ANTI_IDLE_INTERVAL_DEFAULT,
            ssh_keep_alive_interval: SSH_KEEP_ALIVE_INTERVAL_DEFAULT,
            ssh_tmout_reset_interval: SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            rdp_defaults: RdpDefaults::default(),
            external_editor: String::new(),
            putty_path: String::new(),
            ssh_default_mode: SshMode::default(),
            x11_server_path: String::new(),
            x11_auto_start: true,
            ssh_agent_preference: AgentPreference::default(),
            powershell_execution_policy: ExecutionPolicy::default(),
            ctrl_v_paste: CtrlVPaste::default(),
            ctrl_k_terminal: CtrlKTerminal::default(),
            collapse_tunnels_panel: true,
            prevent_sleep: true,
            max_sessions: MAX_SESSIONS_DEFAULT,
            diagnostics_log: true,
            reachability: Reachability::default(),
            sftp_browser: SftpBrowser::default(),
            show_gateway_badge: true,
            updates: Updates::default(),
            update_check: UpdateCheck::default(),
            windows_hello: WindowsHello::default(),
            last_used_gateway: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    terminal: TerminalSection,
    #[serde(default)]
    session_log: SessionLogSection,
    #[serde(default)]
    general: GeneralSection,
    #[serde(default)]
    vault_unlock: VaultUnlockSection,
    #[serde(default)]
    vault: VaultSection,
    #[serde(default)]
    pin: PinSection,
    #[serde(default)]
    credential_provider: ProviderSection,
    #[serde(default)]
    ssh: SshSection,
    #[serde(default)]
    rdp: RdpDefaults,
    #[serde(default)]
    rdp_session: RdpSessionSection,
    #[serde(default)]
    files: FilesSection,
    #[serde(default)]
    reachability: ReachabilitySection,
    #[serde(default)]
    updates: UpdatesSection,
    #[serde(default)]
    update_check: UpdateCheckSection,
    #[serde(default)]
    windows_hello: WindowsHelloSection,
    #[serde(default)]
    profile_form: ProfileFormSection,
}

/// This computer's own: never exported.
#[derive(Serialize, Deserialize, Default)]
struct ProfileFormSection {
    /// The identifier of the gateway; absent when the last profile saved had none.
    #[serde(default)]
    last_used_gateway: Option<String>,
}

impl ProfileFormSection {
    fn of(last_used_gateway: Option<&ProfileId>) -> Self {
        Self {
            last_used_gateway: last_used_gateway.map(|id| id.as_str().to_owned()),
        }
    }
}

/// Absent values are the C# defaults.
#[derive(Serialize, Deserialize, Default)]
struct WindowsHelloSection {
    #[serde(default)]
    require_on_connect: Option<bool>,
    #[serde(default)]
    grace_minutes: Option<u32>,
    #[serde(default)]
    vault_max_days: Option<u32>,
}

/// Absent values are the C# defaults.
#[derive(Serialize, Deserialize, Default)]
struct UpdatesSection {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    interval_hours: Option<u32>,
}

/// This computer's own: never exported.
#[derive(Serialize, Deserialize, Default)]
struct UpdateCheckSection {
    /// Seconds since 1970, UTC.
    #[serde(default)]
    last_check: Option<u64>,
    #[serde(default)]
    skipped: Option<String>,
}

/// Absent values are the C# defaults.
#[derive(Serialize, Deserialize, Default)]
struct ReachabilitySection {
    #[serde(default)]
    enabled: Option<bool>,
    /// Seconds.
    #[serde(default)]
    interval: Option<u32>,
    /// Milliseconds.
    #[serde(default)]
    timeout: Option<u32>,
    #[serde(default)]
    probes: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
struct RdpSessionSection {
    #[serde(default)]
    auto_reconnect_attempts: Option<u32>,
    /// Seconds, 0 for no limit.
    #[serde(default)]
    connect_timeout: Option<u32>,
    /// One `WIDTHxHEIGHT` per preset.
    #[serde(default)]
    resolution_presets: Option<Vec<String>>,
}

/// Absent flags are the C# defaults.
#[derive(Serialize, Deserialize, Default)]
struct FilesSection {
    #[serde(default)]
    external_editor: String,
    #[serde(default)]
    browser_enabled: Option<bool>,
    #[serde(default)]
    auto_open_on_ssh: Option<bool>,
    #[serde(default)]
    follow_ssh_directory: Option<bool>,
    #[serde(default)]
    dock_local_browser: Option<bool>,
    #[serde(default)]
    follow_local_directory: Option<bool>,
}

impl FilesSection {
    /// The SFTP browser's settings the section holds, each absent one its default.
    fn sftp_browser(&self) -> SftpBrowser {
        let defaults = SftpBrowser::default();
        SftpBrowser {
            enabled: self.browser_enabled.unwrap_or(defaults.enabled),
            auto_open_on_ssh: self.auto_open_on_ssh.unwrap_or(defaults.auto_open_on_ssh),
            follow_ssh_directory: self
                .follow_ssh_directory
                .unwrap_or(defaults.follow_ssh_directory),
            dock_local_browser: self
                .dock_local_browser
                .unwrap_or(defaults.dock_local_browser),
            follow_local_directory: self
                .follow_local_directory
                .unwrap_or(defaults.follow_local_directory),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SshSection {
    #[serde(default)]
    auto_reconnect: bool,
    #[serde(default)]
    auto_reconnect_attempts: Option<u32>,
    /// Kept beside the SSH settings, where the C# Settings page shows it.
    #[serde(default)]
    anti_idle_interval: Option<u32>,
    #[serde(default)]
    keep_alive_interval: Option<u32>,
    #[serde(default)]
    tmout_reset_interval: Option<u32>,
    /// The C# name of the agent preference.
    #[serde(default)]
    agent_preference: Option<String>,
    #[serde(default)]
    putty_path: String,
    /// The C# name of the default SSH mode; absent is the C# default, `Embedded`.
    #[serde(default)]
    default_mode: Option<String>,
    #[serde(default)]
    x11_server_path: String,
    /// Absent is the C# default: on.
    #[serde(default)]
    x11_auto_start: Option<bool>,
}

#[derive(Serialize, Deserialize, Default)]
struct ProviderSection {
    #[serde(default)]
    enabled: bool,
    /// The C# name of the kind.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    username_command: String,
    #[serde(default)]
    database: String,
    #[serde(default)]
    key_file: String,
    #[serde(default)]
    first_line_only: bool,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

impl ProviderSection {
    fn settings(self) -> ProviderSettings {
        let defaults = ProviderSettings::default();
        ProviderSettings {
            enabled: self.enabled,
            kind: self
                .kind
                .as_deref()
                .map(ProviderKind::named)
                .unwrap_or_default(),
            command: self.command,
            username_command: self.username_command,
            database: self.database,
            key_file: self.key_file,
            first_line_only: self.first_line_only,
            // Out of the range, as the C# load keeps the default.
            timeout: self
                .timeout_ms
                .map(Duration::from_millis)
                .filter(|timeout| (MIN_TIMEOUT..=MAX_TIMEOUT).contains(timeout))
                .unwrap_or(defaults.timeout),
        }
    }

    fn of(settings: &ProviderSettings) -> Self {
        Self {
            enabled: settings.enabled,
            kind: Some(settings.kind.name().to_owned()),
            command: settings.command.clone(),
            username_command: settings.username_command.clone(),
            database: settings.database.clone(),
            key_file: settings.key_file.clone(),
            first_line_only: settings.first_line_only,
            timeout_ms: Some(u64::try_from(settings.timeout.as_millis()).unwrap_or(u64::MAX)),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct PinSection {
    /// Base64; with the hash, a PIN is set.
    #[serde(default)]
    salt: Option<String>,
    /// Base64 of the Argon2id hash.
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    failures: u32,
    /// Seconds since 1970, UTC.
    #[serde(default)]
    locked_until: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
struct VaultUnlockSection {
    #[serde(default)]
    failures: u32,
    /// Seconds since 1970, UTC.
    #[serde(default)]
    locked_until: Option<u64>,
    /// Seconds since 1970, UTC.
    #[serde(default)]
    last_master_unlock: Option<u64>,
}

/// The workspace lock's preferences; apart from the tries above, which stay on this
/// computer. Absent values are the C# defaults.
#[derive(Serialize, Deserialize, Default)]
struct VaultSection {
    #[serde(default)]
    auto_lock_idle_minutes: Option<u32>,
    #[serde(default)]
    disconnect_on_lock: Option<bool>,
}

#[derive(Serialize, Deserialize, Default)]
struct GeneralSection {
    /// Written only once chosen, as TOML leaves an absent value out: until then the
    /// desktop's language is followed.
    #[serde(default)]
    language: Option<String>,
    /// The window's theme by its C# name; absent is the C# default, Drakul.
    #[serde(default)]
    theme: Option<String>,
    /// The window's accent by its C# name; absent is the theme's own.
    #[serde(default)]
    accent: Option<String>,
    /// Absent is the C# default: collapsed.
    #[serde(default)]
    collapse_tunnels_panel: Option<bool>,
    /// Absent is the C# default: on.
    #[serde(default)]
    prevent_sleep: Option<bool>,
    /// Absent is no limit.
    #[serde(default)]
    max_sessions: Option<u32>,
    /// Absent is the C# default: written.
    #[serde(default)]
    diagnostics_log: Option<bool>,
    /// Absent, from a file written before it, is the C# default: shown.
    #[serde(default)]
    show_gateway_badge: Option<bool>,
}

#[derive(Serialize, Deserialize, Default)]
struct SessionLogSection {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    directory: Option<String>,
    /// Absent, from a file written before it, is the C# default: every transcript kept.
    #[serde(default)]
    retention_days: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
struct TerminalSection {
    #[serde(default)]
    color_scheme: Option<String>,
    #[serde(default)]
    broadcast_scope: Option<String>,
    #[serde(default)]
    font_size: Option<u16>,
    /// Absent, from a file written before it, is the default family.
    #[serde(default)]
    font_family: Option<String>,
    /// The C# name of the local `PowerShell` execution policy.
    #[serde(default)]
    powershell_execution_policy: Option<String>,
    /// What Ctrl+V does, by its name.
    #[serde(default)]
    ctrl_v_paste: Option<String>,
    /// What Ctrl+K does, by its name.
    #[serde(default)]
    ctrl_k: Option<String>,
}

/// The instant `seconds` after 1970, UTC; `None` past what this system's clock can say,
/// a value no run wrote.
fn from_epoch(seconds: u64) -> Option<SystemTime> {
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// The end of a lockout saved as `seconds` after 1970, read at `now`. Past what the clock can
/// say, it fails closed: a full lockout from `now`, never none.
fn lockout_until(seconds: u64, now: SystemTime) -> SystemTime {
    from_epoch(seconds).unwrap_or(now + LOCKOUT_DURATION)
}

/// Seconds from 1970, UTC, to `instant`; 0 for an instant before.
fn to_epoch(instant: SystemTime) -> u64 {
    instant
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The settings file of the profile file `profiles_file`: beside it.
#[must_use]
pub fn settings_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(SETTINGS_FILE_NAME)
}

impl Settings {
    /// The sizes the Resolution menus offer: the presets chosen, else the built-in ones, as
    /// the C# `ResolutionPresetCatalog`.
    #[must_use]
    pub fn resolution_presets(&self) -> &[(u16, u16)] {
        if self.rdp_resolution_presets.is_empty() {
            &RESOLUTION_PRESETS
        } else {
            &self.rdp_resolution_presets
        }
    }

    /// Whether `presets` can be the Resolution menus' presets: each within the limits.
    #[must_use]
    pub fn resolution_presets_accepted(presets: &[(u16, u16)]) -> bool {
        presets.iter().copied().all(preset_fits)
    }

    /// The RDP settings back to their own values, as the C# "Reset RDP defaults": the options
    /// profiles following the application take, the auto-reconnect attempts and the
    /// resolution presets. Nothing else changes, and no profile.
    pub fn reset_rdp(&mut self) {
        let defaults = Self::default();
        self.rdp_defaults = defaults.rdp_defaults;
        self.rdp_auto_reconnect_attempts = defaults.rdp_auto_reconnect_attempts;
        self.rdp_connect_timeout = defaults.rdp_connect_timeout;
        self.rdp_resolution_presets = defaults.rdp_resolution_presets;
    }

    /// Every setting back to its default, as the C# "Reset defaults"
    /// (`SettingsViewModel.cs:2376-2464`), but what the C# carries across: the language,
    /// the theme and the accent, which a reader of another language needs to find the way
    /// back; the PIN and the lockouts, which are state, not preferences; the release skipped
    /// and the last look for one; and what no C# Settings panel edits (the tree's gateway
    /// badge, the broadcast scope, the gateway a new profile starts on). The gateways, the
    /// profiles, the master password, the Windows Hello enrolment, the macros and the
    /// credential provider's unlock secret are kept elsewhere and are not touched.
    pub fn reset_all(&mut self) {
        let kept = Self {
            language: self.language,
            theme: self.theme,
            accent: self.accent,
            vault_unlock: self.vault_unlock,
            vault_last_master_unlock: self.vault_last_master_unlock,
            pin: self.pin.take(),
            pin_unlock: self.pin_unlock,
            broadcast_scope: self.broadcast_scope,
            show_gateway_badge: self.show_gateway_badge,
            update_check: std::mem::take(&mut self.update_check),
            last_used_gateway: self.last_used_gateway.take(),
            ..Self::default()
        };
        *self = kept;
    }

    /// Reads the settings at `path`; a missing file holds the defaults.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// a newer format version.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let file: SettingsFile = toml::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.to_owned(),
            source,
        })?;
        if file.version > SETTINGS_FILE_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path: path.to_owned(),
                found: file.version,
                expected: SETTINGS_FILE_VERSION,
            });
        }
        Ok(Self::from_file(file))
    }

    /// The settings `file` says; a value out of its range is the default.
    #[expect(clippy::too_many_lines, reason = "one field of the file per setting")]
    fn from_file(file: SettingsFile) -> Self {
        Self {
            color_scheme: file
                .terminal
                .color_scheme
                .as_deref()
                .map(ColorScheme::named)
                .unwrap_or_default(),
            broadcast_scope: file
                .terminal
                .broadcast_scope
                .as_deref()
                .map(BroadcastScope::named)
                .unwrap_or_default(),
            session_logging: file.session_log.enabled,
            session_log_directory: file
                .session_log
                .directory
                .filter(|directory| !directory.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SESSION_LOG_DIRECTORY.to_owned()),
            // Out of the range, as the C# load warns and keeps the default.
            session_log_retention_days: within(
                file.session_log.retention_days,
                session_log_retention_days_accepted,
                SESSION_LOG_RETENTION_DAYS_DEFAULT,
            ),
            // Out of the range, as the C# load warns and keeps the default.
            terminal_font_size: file
                .terminal
                .font_size
                .filter(|size| terminal_font_size_accepted(*size))
                .unwrap_or(TERMINAL_FONT_SIZE_DEFAULT),
            terminal_font_family: terminal_font_family(
                file.terminal.font_family.as_deref().unwrap_or_default(),
            ),
            vault_unlock: Lockout::restored(
                file.vault_unlock.failures,
                file.vault_unlock
                    .locked_until
                    .map(|seconds| lockout_until(seconds, SystemTime::now())),
                SystemTime::now(),
            ),
            // Past what the clock can say, never typed: the master password is asked.
            vault_last_master_unlock: file.vault_unlock.last_master_unlock.and_then(from_epoch),
            // Out of the range, as the C# load warns and keeps the default.
            auto_lock_idle_minutes: within(
                file.vault.auto_lock_idle_minutes,
                auto_lock_idle_minutes_accepted,
                AUTO_LOCK_IDLE_MINUTES_DEFAULT,
            ),
            disconnect_on_lock: file.vault.disconnect_on_lock.unwrap_or_default(),
            // Half a PIN is still one: it takes nothing, and the application stays closed.
            pin: match (file.pin.salt, file.pin.hash) {
                (None, None) => None,
                (salt, hash) => Some(PinHash::saved(
                    salt.unwrap_or_default(),
                    hash.unwrap_or_default(),
                )),
            },
            pin_unlock: Lockout::restored(
                file.pin.failures,
                file.pin
                    .locked_until
                    .map(|seconds| lockout_until(seconds, SystemTime::now())),
                SystemTime::now(),
            ),
            credential_provider: file.credential_provider.settings(),
            ssh_auto_reconnect: file.ssh.auto_reconnect,
            // Out of the range, as the C# load warns and keeps the default.
            rdp_auto_reconnect_attempts: within(
                file.rdp_session.auto_reconnect_attempts,
                rdp_auto_reconnect_attempts_accepted,
                RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
            ),
            rdp_connect_timeout: within(
                file.rdp_session.connect_timeout,
                rdp_connect_timeout_accepted,
                RDP_CONNECT_TIMEOUT_DEFAULT,
            ),
            // A line that is not a preset is left out, as the C# menu leaves it out.
            rdp_resolution_presets: file.rdp_session.resolution_presets.map_or_else(
                || RESOLUTION_PRESETS.to_vec(),
                |lines| {
                    lines
                        .iter()
                        .filter_map(|line| resolution_preset(line))
                        .collect()
                },
            ),
            ssh_auto_reconnect_attempts: within(
                file.ssh.auto_reconnect_attempts,
                ssh_auto_reconnect_attempts_accepted,
                SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
            ),
            anti_idle_interval: within(
                file.ssh.anti_idle_interval,
                anti_idle_interval_accepted,
                ANTI_IDLE_INTERVAL_DEFAULT,
            ),
            ssh_keep_alive_interval: within(
                file.ssh.keep_alive_interval,
                ssh_keep_alive_interval_accepted,
                SSH_KEEP_ALIVE_INTERVAL_DEFAULT,
            ),
            ssh_tmout_reset_interval: within(
                file.ssh.tmout_reset_interval,
                ssh_tmout_reset_interval_accepted,
                SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            ),
            ssh_agent_preference: file
                .ssh
                .agent_preference
                .as_deref()
                .map(AgentPreference::named)
                .unwrap_or_default(),
            powershell_execution_policy: file
                .terminal
                .powershell_execution_policy
                .as_deref()
                .map(ExecutionPolicy::named)
                .unwrap_or_default(),
            ctrl_v_paste: file
                .terminal
                .ctrl_v_paste
                .as_deref()
                .map(CtrlVPaste::named)
                .unwrap_or_default(),
            ctrl_k_terminal: file
                .terminal
                .ctrl_k
                .as_deref()
                .map(CtrlKTerminal::named)
                .unwrap_or_default(),
            rdp_defaults: file.rdp,
            sftp_browser: file.files.sftp_browser(),
            external_editor: file.files.external_editor.trim().to_owned(),
            putty_path: file.ssh.putty_path.trim().to_owned(),
            ssh_default_mode: file
                .ssh
                .default_mode
                .as_deref()
                .map(SshMode::named)
                .unwrap_or_default(),
            x11_server_path: file.ssh.x11_server_path.trim().to_owned(),
            x11_auto_start: file.ssh.x11_auto_start.unwrap_or(true),
            collapse_tunnels_panel: file.general.collapse_tunnels_panel.unwrap_or(true),
            prevent_sleep: file.general.prevent_sleep.unwrap_or(true),
            max_sessions: within(
                file.general.max_sessions,
                max_sessions_accepted,
                MAX_SESSIONS_DEFAULT,
            ),
            diagnostics_log: file.general.diagnostics_log.unwrap_or(true),
            show_gateway_badge: file.general.show_gateway_badge.unwrap_or(true),
            updates: Updates {
                enabled: file.updates.enabled.unwrap_or(true),
                interval_hours: within(
                    file.updates.interval_hours,
                    update_interval_accepted,
                    UPDATE_INTERVAL_HOURS_DEFAULT,
                ),
            },
            update_check: UpdateCheck {
                last_check: file.update_check.last_check.and_then(from_epoch),
                skipped: file
                    .update_check
                    .skipped
                    .filter(|tag| !tag.trim().is_empty()),
            },
            windows_hello: WindowsHello {
                require_on_connect: file.windows_hello.require_on_connect.unwrap_or_default(),
                // Out of the range, as the C# load warns and keeps the default.
                grace_minutes: within(
                    file.windows_hello.grace_minutes,
                    windows_hello_grace_minutes_accepted,
                    WINDOWS_HELLO_GRACE_MINUTES_DEFAULT,
                ),
                vault_max_days: within(
                    file.windows_hello.vault_max_days,
                    windows_hello_vault_max_days_accepted,
                    WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER,
                ),
            },
            last_used_gateway: file
                .profile_form
                .last_used_gateway
                .filter(|id| !id.trim().is_empty())
                .map(ProfileId::new),
            reachability: Reachability {
                enabled: file.reachability.enabled.unwrap_or(true),
                interval: within(
                    file.reachability.interval,
                    reachability_interval_accepted,
                    REACHABILITY_INTERVAL_DEFAULT,
                ),
                timeout: within(
                    file.reachability.timeout,
                    reachability_timeout_accepted,
                    REACHABILITY_TIMEOUT_DEFAULT,
                ),
                probes: within(
                    file.reachability.probes,
                    reachability_probes_accepted,
                    REACHABILITY_PROBES_DEFAULT,
                ),
            },
            // A language not offered is not guessed: the desktop's is followed.
            language: file
                .general
                .language
                .as_deref()
                .and_then(Language::from_code),
            theme: file
                .general
                .theme
                .as_deref()
                .map(AppTheme::named)
                .unwrap_or_default(),
            accent: file
                .general
                .accent
                .as_deref()
                .map(Accent::named)
                .unwrap_or_default(),
        }
    }

    /// The folder transcripts go to: the one chosen when absolute, else under the folder of
    /// `settings_file`.
    #[must_use]
    pub fn session_log_folder(&self, settings_file: &Path) -> PathBuf {
        // Rebuilt from its parts: the default "logs/sessions" takes the platform's separator.
        let chosen: PathBuf = Path::new(self.session_log_directory.trim())
            .components()
            .collect();
        if chosen.is_absolute() {
            return chosen;
        }
        settings_file
            .parent()
            .map_or_else(|| chosen.clone(), |base| base.join(&chosen))
    }

    /// Writes the settings to `path`, through a temporary file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&self.file())?;
        write_atomic(path, &text)
    }

    /// The settings as their file holds them.
    fn file(&self) -> SettingsFile {
        SettingsFile {
            version: SETTINGS_FILE_VERSION,
            terminal: TerminalSection {
                color_scheme: Some(self.color_scheme.name().to_owned()),
                broadcast_scope: Some(self.broadcast_scope.name().to_owned()),
                font_size: Some(self.terminal_font_size),
                font_family: Some(self.terminal_font_family.clone()),
                powershell_execution_policy: Some(
                    self.powershell_execution_policy.name().to_owned(),
                ),
                ctrl_v_paste: Some(self.ctrl_v_paste.name().to_owned()),
                ctrl_k: Some(self.ctrl_k_terminal.name().to_owned()),
            },
            session_log: SessionLogSection {
                enabled: self.session_logging,
                directory: Some(self.session_log_directory.clone()),
                retention_days: Some(self.session_log_retention_days),
            },
            general: GeneralSection {
                language: self.language.map(|language| language.code().to_owned()),
                theme: Some(self.theme.name().to_owned()),
                accent: Some(self.accent.name().to_owned()),
                collapse_tunnels_panel: Some(self.collapse_tunnels_panel),
                prevent_sleep: Some(self.prevent_sleep),
                max_sessions: Some(self.max_sessions),
                diagnostics_log: Some(self.diagnostics_log),
                show_gateway_badge: Some(self.show_gateway_badge),
            },
            vault_unlock: VaultUnlockSection {
                failures: self.vault_unlock.failures(),
                locked_until: self.vault_unlock.until().map(to_epoch),
                last_master_unlock: self.vault_last_master_unlock.map(to_epoch),
            },
            vault: VaultSection {
                auto_lock_idle_minutes: Some(self.auto_lock_idle_minutes),
                disconnect_on_lock: Some(self.disconnect_on_lock),
            },
            pin: PinSection {
                salt: self.pin.as_ref().map(|pin| pin.salt().to_owned()),
                hash: self.pin.as_ref().map(|pin| pin.hash().to_owned()),
                failures: self.pin_unlock.failures(),
                locked_until: self.pin_unlock.until().map(to_epoch),
            },
            credential_provider: ProviderSection::of(&self.credential_provider),
            ssh: SshSection {
                auto_reconnect: self.ssh_auto_reconnect,
                auto_reconnect_attempts: Some(self.ssh_auto_reconnect_attempts),
                anti_idle_interval: Some(self.anti_idle_interval),
                keep_alive_interval: Some(self.ssh_keep_alive_interval),
                tmout_reset_interval: Some(self.ssh_tmout_reset_interval),
                agent_preference: Some(self.ssh_agent_preference.name().to_owned()),
                putty_path: self.putty_path.clone(),
                default_mode: Some(self.ssh_default_mode.name().to_owned()),
                x11_server_path: self.x11_server_path.clone(),
                x11_auto_start: Some(self.x11_auto_start),
            },
            rdp: self.rdp_defaults,
            rdp_session: RdpSessionSection {
                auto_reconnect_attempts: Some(self.rdp_auto_reconnect_attempts),
                connect_timeout: Some(self.rdp_connect_timeout),
                resolution_presets: Some(
                    self.rdp_resolution_presets
                        .iter()
                        .copied()
                        .map(resolution_text)
                        .collect(),
                ),
            },
            files: FilesSection {
                external_editor: self.external_editor.clone(),
                browser_enabled: Some(self.sftp_browser.enabled),
                auto_open_on_ssh: Some(self.sftp_browser.auto_open_on_ssh),
                follow_ssh_directory: Some(self.sftp_browser.follow_ssh_directory),
                dock_local_browser: Some(self.sftp_browser.dock_local_browser),
                follow_local_directory: Some(self.sftp_browser.follow_local_directory),
            },
            reachability: ReachabilitySection {
                enabled: Some(self.reachability.enabled),
                interval: Some(self.reachability.interval),
                timeout: Some(self.reachability.timeout),
                probes: Some(self.reachability.probes),
            },
            updates: UpdatesSection {
                enabled: Some(self.updates.enabled),
                interval_hours: Some(self.updates.interval_hours),
            },
            update_check: UpdateCheckSection {
                last_check: self.update_check.last_check.map(to_epoch),
                skipped: self.update_check.skipped.clone(),
            },
            windows_hello: WindowsHelloSection {
                require_on_connect: Some(self.windows_hello.require_on_connect),
                grace_minutes: Some(self.windows_hello.grace_minutes),
                vault_max_days: Some(self.windows_hello.vault_max_days),
            },
            profile_form: ProfileFormSection::of(self.last_used_gateway.as_ref()),
        }
    }

    /// The sections of the settings file that travel, as a table: every preference, none of
    /// the PIN, the master password's tries or the last look for a newer release.
    fn transferable(&self) -> toml::Table {
        toml::Table::try_from(self.file())
            .unwrap_or_default()
            .into_iter()
            .filter(|(section, _)| TRANSFERRED.contains(&section.as_str()))
            .collect()
    }

    /// The portable settings file, as the C# "Export settings": every preference, nothing
    /// secret and nothing of this computer's PIN, lockouts or update checks. A value naming
    /// a path under
    /// `home` belongs to this computer's user and stays behind unless `with_home`; how many
    /// stayed is returned with the text.
    #[must_use]
    pub fn export(&self, home: Option<&Path>, with_home: bool) -> (String, usize) {
        let mut sections = self.transferable();
        let mut held_back = 0;
        if let Some(home) = home.filter(|_| !with_home) {
            for (_, section) in &mut sections {
                if let toml::Value::Table(values) = section {
                    values.retain(|_, value| {
                        let names = value.as_str().is_some_and(|text| under(text, home));
                        held_back += usize::from(names);
                        !names
                    });
                }
            }
        }
        let mut document = toml::Table::new();
        document.insert(
            TRANSFER_FORMAT_KEY.to_owned(),
            toml::Value::String(TRANSFER_FORMAT.to_owned()),
        );
        document.insert(
            TRANSFER_VERSION_KEY.to_owned(),
            toml::Value::Integer(TRANSFER_VERSION),
        );
        document.insert(
            TRANSFER_SETTINGS_KEY.to_owned(),
            toml::Value::Table(sections),
        );
        (
            toml::to_string_pretty(&document).unwrap_or_default(),
            held_back,
        )
    }

    /// These settings with those of the portable settings file `text` laid over them, as the
    /// C# "Import settings", and what that changes. Only the preferences an export carries
    /// are taken: a PIN or a lockout written in by hand is not. A value out of its range is
    /// the default, as when the settings are read.
    ///
    /// # Errors
    ///
    /// [`TransferError`] when `text` is not a settings file this version reads.
    pub fn import(&self, text: &str) -> Result<SettingsImport, TransferError> {
        let document: toml::Table = text.parse().map_err(|_| TransferError::NotSettings)?;
        if document
            .get(TRANSFER_FORMAT_KEY)
            .and_then(toml::Value::as_str)
            != Some(TRANSFER_FORMAT)
        {
            return Err(TransferError::NotSettings);
        }
        match document
            .get(TRANSFER_VERSION_KEY)
            .and_then(toml::Value::as_integer)
        {
            Some(TRANSFER_VERSION) => {}
            Some(found) if found > TRANSFER_VERSION => return Err(TransferError::Newer),
            _ => return Err(TransferError::NotSettings),
        }
        let Some(toml::Value::Table(incoming)) = document.get(TRANSFER_SETTINGS_KEY) else {
            return Err(TransferError::NotSettings);
        };
        let mut merged = toml::Table::try_from(self.file()).unwrap_or_default();
        for (section, values) in incoming {
            let toml::Value::Table(values) = values else {
                continue;
            };
            if !TRANSFERRED.contains(&section.as_str()) {
                continue;
            }
            let target = merged
                .entry(section.clone())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(target) = target {
                for (key, value) in values {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        let file: SettingsFile = merged.try_into().map_err(|_| TransferError::NotSettings)?;
        let settings = Self::from_file(file);
        let changes = changes(&self.transferable(), &settings.transferable());
        Ok(SettingsImport { settings, changes })
    }
}

/// What a portable settings file says it is.
const TRANSFER_FORMAT: &str = "heimdall-settings";

/// The version of the portable settings file this build writes and reads.
const TRANSFER_VERSION: i64 = 1;

/// Its keys.
const TRANSFER_FORMAT_KEY: &str = "format";
const TRANSFER_VERSION_KEY: &str = "version";
const TRANSFER_SETTINGS_KEY: &str = "settings";

/// The sections of the settings file a portable settings file carries.
const TRANSFERRED: [&str; 12] = [
    "terminal",
    "session_log",
    "general",
    "vault",
    "credential_provider",
    "ssh",
    "rdp",
    "rdp_session",
    "files",
    "reachability",
    "updates",
    "windows_hello",
];

/// The name a portable settings file is offered under.
pub const SETTINGS_EXPORT_FILE_NAME: &str = "heimdall-settings.toml";

/// Settings read from a portable settings file, and what they change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsImport {
    /// The settings with the file's laid over them.
    pub settings: Settings,
    /// Each setting the file changes, in the file's order of sections and keys.
    pub changes: Vec<SettingChange>,
}

/// One setting a portable settings file changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingChange {
    /// Its section and key, as the file names it: `ssh.keep_alive_interval`.
    pub key: String,
    /// Its value now; `None` when it has none.
    pub before: Option<SettingValue>,
    /// Its value from the file; `None` when it has none.
    pub after: Option<SettingValue>,
}

/// A setting's value, as a change shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    /// On or off.
    Flag(bool),
    /// Text, a number among them; empty text is shown as such.
    Text(String),
    /// A list of this many items.
    List(usize),
}

impl SettingValue {
    fn of(value: &toml::Value) -> Self {
        match value {
            toml::Value::Boolean(on) => Self::Flag(*on),
            toml::Value::String(text) => Self::Text(text.clone()),
            toml::Value::Array(items) => Self::List(items.len()),
            other => Self::Text(other.to_string()),
        }
    }
}

/// Why a file is not imported as settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    /// It is not a portable settings file, or not one that can be read.
    NotSettings,
    /// It was written by a newer version.
    Newer,
}

/// Whether the path `text` is under `home`, whatever the case of either.
fn under(text: &str, home: &Path) -> bool {
    let text = text.trim().to_lowercase();
    let home = home.to_string_lossy().to_lowercase();
    !home.is_empty() && Path::new(&text).starts_with(Path::new(&home))
}

/// Every value of `after` not the same in `before`, section by section.
fn changes(before: &toml::Table, after: &toml::Table) -> Vec<SettingChange> {
    let empty = toml::Table::new();
    let table = |all: &toml::Table, section: &str| match all.get(section) {
        Some(toml::Value::Table(values)) => values.clone(),
        _ => empty.clone(),
    };
    let mut changed = Vec::new();
    for section in TRANSFERRED {
        let (was, now) = (table(before, section), table(after, section));
        let mut keys: Vec<&String> = was.keys().chain(now.keys()).collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            let (old, new) = (was.get(key), now.get(key));
            if old != new {
                changed.push(SettingChange {
                    key: format!("{section}.{key}"),
                    before: old.map(SettingValue::of),
                    after: new.map(SettingValue::of),
                });
            }
        }
    }
    changed
}

#[cfg(test)]
mod transfer_tests {
    use super::{Settings, TRANSFERRED};

    /// The sections that never travel: this computer's PIN and lockouts, its last look for a
    /// newer release, the gateway its profile form starts on, and the file's own version.
    const HELD_BACK: [&str; 5] = [
        "version",
        "vault_unlock",
        "pin",
        "update_check",
        "profile_form",
    ];

    #[test]
    fn every_section_of_the_settings_file_travels_or_is_held_back() {
        let sections = toml::Table::try_from(Settings::default().file()).expect("table");
        for section in sections.keys() {
            assert!(
                TRANSFERRED.contains(&section.as_str()) || HELD_BACK.contains(&section.as_str()),
                "{section}: travels with an export or not? Say so in TRANSFERRED or HELD_BACK."
            );
        }
    }
}

#[cfg(test)]
mod epoch_tests {
    use std::time::{Duration, SystemTime};

    use super::{LOCKOUT_DURATION, from_epoch, lockout_until};

    #[test]
    fn a_time_past_what_the_clock_can_say_is_absent_and_a_lockout_there_stays_locked() {
        assert_eq!(
            from_epoch(90),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(90))
        );
        assert_eq!(from_epoch(u64::MAX), None, "read, never a panic");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_001);
        // Fail closed: a full lockout from now, rather than none.
        assert_eq!(lockout_until(u64::MAX, now), now + LOCKOUT_DURATION);
        assert_eq!(
            lockout_until(1_700_000_101, now),
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_101)
        );
    }
}
