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

//! The rows of the Settings page, kept in one list: the card and the tab each is on, whether
//! its value differs from the default, and the change that puts the default back. The page
//! is drawn from this list, its search filters it and its "Modified" markers read it, so a
//! row added here is drawn, found and marked at once, as the C# panel derives its markers
//! from the settings it binds.
//!
//! With them, the security overview of the C# Security tab: the security-relevant choices,
//! each with its state and whether that state is the documented insecure one.

use heimdall_app::SettingsMessage;
use heimdall_core::settings::{AUTO_LOCK_IDLE_MINUTES_OFF, ExecutionPolicy, Settings, SftpBrowser};

use crate::session_settings::SessionField;
use crate::shell::SettingsTab;

/// A card of the Settings page: the rows under one heading, on one tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCard {
    /// The application's language, theme and accent.
    Appearance,
    /// The tunnels panel, the computer's sleep, the most sessions.
    Behavior,
    /// The background check of every server.
    Reachability,
    /// The terminals' text and colours, Ctrl+V and the `PowerShell` policy.
    Terminal,
    /// The session transcripts.
    SessionLogging,
    /// The terminal macros kept.
    Macros,
    /// SSH auto-reconnect and the SSH agent preferred.
    SshReconnect,
    /// The SSH keep-alive, `TMOUT` reset and anti-idle numbers.
    SshSession,
    /// `PuTTY`, which SSH profiles in the external mode open in.
    Putty,
    /// The SFTP browser and the local file browser.
    Sftp,
    /// The X server X11 forwarding draws on.
    X11,
    /// The program a server's file is edited with.
    ExternalEditor,
    /// The trusted SSH host keys and FTPS certificates.
    SshTrusted,
    /// The RDP options profiles following the application's take.
    RdpDefaults,
    /// The RDP auto-reconnect attempts and the logon watchdog.
    RdpSession,
    /// The sizes the RDP Resolution menus offer.
    RdpPresets,
    /// The RDP settings put back at once.
    RdpReset,
    /// The trusted RDP certificates.
    RdpTrusted,
    /// The SSH gateways.
    Gateways,
    /// The application PIN.
    Pin,
    /// The master password.
    Vault,
    /// The external credential provider.
    Provider,
}

impl SettingsCard {
    /// Every card, in the page's order.
    pub const ALL: [Self; 22] = [
        Self::Appearance,
        Self::Behavior,
        Self::Reachability,
        Self::Terminal,
        Self::SessionLogging,
        Self::Macros,
        Self::SshReconnect,
        Self::SshSession,
        Self::Putty,
        Self::Sftp,
        Self::X11,
        Self::ExternalEditor,
        Self::SshTrusted,
        Self::RdpDefaults,
        Self::RdpSession,
        Self::RdpPresets,
        Self::RdpReset,
        Self::RdpTrusted,
        Self::Gateways,
        Self::Pin,
        Self::Vault,
        Self::Provider,
    ];

    /// The tab it is on.
    #[must_use]
    pub fn tab(self) -> SettingsTab {
        match self {
            Self::Appearance | Self::Behavior | Self::Reachability => SettingsTab::General,
            Self::Terminal | Self::SessionLogging | Self::Macros => SettingsTab::Terminal,
            Self::SshReconnect
            | Self::SshSession
            | Self::Putty
            | Self::Sftp
            | Self::X11
            | Self::ExternalEditor
            | Self::SshTrusted => SettingsTab::Ssh,
            Self::RdpDefaults
            | Self::RdpSession
            | Self::RdpPresets
            | Self::RdpReset
            | Self::RdpTrusted => SettingsTab::Rdp,
            Self::Gateways => SettingsTab::Gateways,
            Self::Pin | Self::Vault | Self::Provider => SettingsTab::Security,
        }
    }

    /// Its rows, in the page's order.
    #[must_use]
    pub fn rows(self) -> Vec<SettingRow> {
        SettingRow::ALL
            .into_iter()
            .filter(|row| row.card() == self)
            .collect()
    }
}

/// A row of the Settings page: one setting, or a list or card that holds several and is
/// found and shown whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingRow {
    /// The language of the window.
    Language,
    /// The window's theme.
    Theme,
    /// The window's accent.
    Accent,
    /// The tunnels panel starts collapsed.
    CollapseTunnelsPanel,
    /// The computer kept from sleeping while a session is open.
    PreventSleep,
    /// Most sessions open at once.
    MaxSessions,
    /// The background check of every server runs.
    Reachability,
    /// Seconds between two background checks.
    ReachabilityInterval,
    /// Milliseconds a server has to answer one.
    ReachabilityTimeout,
    /// Servers checked at once.
    ReachabilityProbes,
    /// The size of the terminals' text.
    FontSize,
    /// The family of the terminals' text.
    FontFamily,
    /// The terminals' colours.
    ColorScheme,
    /// What Ctrl+V does in a terminal.
    CtrlVPaste,
    /// The execution policy a local `PowerShell` is started with.
    PowerShellPolicy,
    /// Every session keeps a transcript.
    SessionLogging,
    /// Where the transcripts go.
    SessionLogDirectory,
    /// Days a transcript is kept.
    TranscriptRetention,
    /// The terminal macros kept.
    Macros,
    /// An SSH session lost opens again by itself.
    SshAutoReconnect,
    /// Attempts of an SSH auto-reconnect.
    SshAutoReconnectAttempts,
    /// Which SSH agent's keys are offered first.
    SshAgentPreference,
    /// Seconds between two SSH keep-alives.
    KeepAlive,
    /// Seconds of no input before an SSH shell's `TMOUT` reset.
    TmoutReset,
    /// Seconds between two anti-idle keys of an RDP session.
    AntiIdle,
    /// `PuTTY`, which SSH profiles in the external mode open in.
    PuttyPath,
    /// The SFTP browser is on.
    SftpBrowser,
    /// An SSH shell connected opens its files beside it.
    SftpAutoOpen,
    /// The SFTP pane follows the shell's working folder.
    SftpFollow,
    /// A local shell gets a file browser beside it.
    DockLocalBrowser,
    /// That browser follows the shell's working folder.
    LocalFollow,
    /// The X server started for X11 forwarding.
    X11ServerPath,
    /// An X server is started when X11 forwarding needs one.
    X11AutoStart,
    /// The program a server's file is edited with.
    ExternalEditor,
    /// The trusted SSH host keys.
    HostKeys,
    /// The trusted FTPS certificates.
    FtpsCertificates,
    /// The RDP options profiles following the application's take.
    RdpDefaults,
    /// Attempts of an RDP auto-reconnect.
    RdpAutoReconnectAttempts,
    /// Seconds an RDP connection may take to log on.
    RdpConnectTimeout,
    /// The sizes the RDP Resolution menus offer.
    RdpResolutionPresets,
    /// "Reset RDP defaults".
    RdpResetAll,
    /// The trusted RDP certificates.
    Certificates,
    /// The SSH gateways.
    Gateways,
    /// The application PIN.
    Pin,
    /// The master password.
    Vault,
    /// Minutes without input before the workspace locks.
    AutoLock,
    /// Locking the workspace closes every session.
    DisconnectOnLock,
    /// The external credential provider.
    Provider,
}

impl SettingRow {
    /// Every row, in the page's order.
    pub const ALL: [Self; 48] = [
        Self::Language,
        Self::Theme,
        Self::Accent,
        Self::CollapseTunnelsPanel,
        Self::PreventSleep,
        Self::MaxSessions,
        Self::Reachability,
        Self::ReachabilityInterval,
        Self::ReachabilityTimeout,
        Self::ReachabilityProbes,
        Self::FontSize,
        Self::FontFamily,
        Self::ColorScheme,
        Self::CtrlVPaste,
        Self::PowerShellPolicy,
        Self::SessionLogging,
        Self::SessionLogDirectory,
        Self::TranscriptRetention,
        Self::Macros,
        Self::SshAutoReconnect,
        Self::SshAutoReconnectAttempts,
        Self::SshAgentPreference,
        Self::KeepAlive,
        Self::TmoutReset,
        Self::AntiIdle,
        Self::PuttyPath,
        Self::SftpBrowser,
        Self::SftpAutoOpen,
        Self::SftpFollow,
        Self::DockLocalBrowser,
        Self::LocalFollow,
        Self::X11ServerPath,
        Self::X11AutoStart,
        Self::ExternalEditor,
        Self::HostKeys,
        Self::FtpsCertificates,
        Self::RdpDefaults,
        Self::RdpAutoReconnectAttempts,
        Self::RdpConnectTimeout,
        Self::RdpResolutionPresets,
        Self::RdpResetAll,
        Self::Certificates,
        Self::Gateways,
        Self::Pin,
        Self::Vault,
        Self::AutoLock,
        Self::DisconnectOnLock,
        Self::Provider,
    ];

    /// The card it is on.
    #[must_use]
    pub fn card(self) -> SettingsCard {
        match self {
            Self::Language | Self::Theme | Self::Accent => SettingsCard::Appearance,
            Self::CollapseTunnelsPanel | Self::PreventSleep | Self::MaxSessions => {
                SettingsCard::Behavior
            }
            Self::Reachability
            | Self::ReachabilityInterval
            | Self::ReachabilityTimeout
            | Self::ReachabilityProbes => SettingsCard::Reachability,
            Self::FontSize
            | Self::FontFamily
            | Self::ColorScheme
            | Self::CtrlVPaste
            | Self::PowerShellPolicy => SettingsCard::Terminal,
            Self::SessionLogging | Self::SessionLogDirectory | Self::TranscriptRetention => {
                SettingsCard::SessionLogging
            }
            Self::Macros => SettingsCard::Macros,
            Self::SshAutoReconnect | Self::SshAutoReconnectAttempts | Self::SshAgentPreference => {
                SettingsCard::SshReconnect
            }
            Self::KeepAlive | Self::TmoutReset | Self::AntiIdle => SettingsCard::SshSession,
            Self::PuttyPath => SettingsCard::Putty,
            Self::SftpBrowser
            | Self::SftpAutoOpen
            | Self::SftpFollow
            | Self::DockLocalBrowser
            | Self::LocalFollow => SettingsCard::Sftp,
            Self::X11ServerPath | Self::X11AutoStart => SettingsCard::X11,
            Self::ExternalEditor => SettingsCard::ExternalEditor,
            Self::HostKeys | Self::FtpsCertificates => SettingsCard::SshTrusted,
            Self::RdpDefaults => SettingsCard::RdpDefaults,
            Self::RdpAutoReconnectAttempts | Self::RdpConnectTimeout => SettingsCard::RdpSession,
            Self::RdpResolutionPresets => SettingsCard::RdpPresets,
            Self::RdpResetAll => SettingsCard::RdpReset,
            Self::Certificates => SettingsCard::RdpTrusted,
            Self::Gateways => SettingsCard::Gateways,
            Self::Pin => SettingsCard::Pin,
            // Under the master password, as the C# `SettingsSectionVault` holds them.
            Self::Vault | Self::AutoLock | Self::DisconnectOnLock => SettingsCard::Vault,
            Self::Provider => SettingsCard::Provider,
        }
    }

    /// The tab it is on.
    #[must_use]
    pub fn tab(self) -> SettingsTab {
        self.card().tab()
    }

    /// The number it is, when it is one typed and applied with Enter.
    #[must_use]
    pub fn session_field(self) -> Option<SessionField> {
        Some(match self {
            Self::ReachabilityInterval => SessionField::ReachabilityInterval,
            Self::ReachabilityTimeout => SessionField::ReachabilityTimeout,
            Self::ReachabilityProbes => SessionField::ReachabilityProbes,
            Self::TranscriptRetention => SessionField::TranscriptRetention,
            Self::KeepAlive => SessionField::KeepAlive,
            Self::TmoutReset => SessionField::TmoutReset,
            Self::AntiIdle => SessionField::AntiIdle,
            Self::AutoLock => SessionField::AutoLock,
            _ => return None,
        })
    }

    /// The program's path it is, when it is one typed and applied with Enter.
    #[must_use]
    pub fn tool_path(self) -> Option<ToolPath> {
        match self {
            Self::PuttyPath => Some(ToolPath::Putty),
            Self::X11ServerPath => Some(ToolPath::X11Server),
            _ => None,
        }
    }

    /// Whether it means something only with a master password set: the workspace lock's
    /// settings, shown disabled without one as the C# shows them.
    #[must_use]
    pub fn needs_vault(self) -> bool {
        matches!(self, Self::AutoLock | Self::DisconnectOnLock)
    }

    /// Whether it carries a "Modified" marker and a reset: a value with a default. The
    /// language follows the desktop unless chosen, which the list cannot offer back, and the
    /// C# keeps it apart from its settings; the lists and cards are inventories (keys,
    /// macros, gateways) or secrets (the PIN, the vault, the provider's secret), as the C#
    /// leaves its external tools and its unlock secret unmarked.
    #[must_use]
    pub fn is_marked(self) -> bool {
        !matches!(
            self,
            Self::Language
                | Self::Macros
                | Self::HostKeys
                | Self::FtpsCertificates
                | Self::RdpResetAll
                | Self::Certificates
                | Self::Gateways
                | Self::Pin
                | Self::Vault
                | Self::Provider
        )
    }

    /// Whether its value in `settings` differs from the default; never for a row without a
    /// marker.
    #[must_use]
    pub fn is_modified(self, settings: &Settings) -> bool {
        let defaults = Settings::default();
        if let Some(field) = self.session_field() {
            return field.value(settings) != field.value(&defaults);
        }
        if let Some(on) = self.flag(settings) {
            return Some(on) != self.flag(&defaults);
        }
        if let Some(path) = self.tool_path() {
            return path.value(settings).trim() != path.value(&defaults);
        }
        match self {
            Self::Theme => settings.theme != defaults.theme,
            Self::Accent => settings.accent != defaults.accent,
            Self::MaxSessions => settings.max_sessions != defaults.max_sessions,
            Self::FontSize => settings.terminal_font_size != defaults.terminal_font_size,
            Self::FontFamily => settings.terminal_font_family != defaults.terminal_font_family,
            Self::ColorScheme => settings.color_scheme != defaults.color_scheme,
            Self::CtrlVPaste => settings.ctrl_v_paste != defaults.ctrl_v_paste,
            Self::PowerShellPolicy => {
                settings.powershell_execution_policy != defaults.powershell_execution_policy
            }
            Self::SessionLogDirectory => {
                settings.session_log_directory.trim() != defaults.session_log_directory
            }
            Self::SshAutoReconnectAttempts => {
                settings.ssh_auto_reconnect_attempts != defaults.ssh_auto_reconnect_attempts
            }
            Self::SshAgentPreference => {
                settings.ssh_agent_preference != defaults.ssh_agent_preference
            }
            Self::ExternalEditor => settings.external_editor.trim() != defaults.external_editor,
            Self::RdpDefaults => settings.rdp_defaults != defaults.rdp_defaults,
            Self::RdpAutoReconnectAttempts => {
                settings.rdp_auto_reconnect_attempts != defaults.rdp_auto_reconnect_attempts
            }
            Self::RdpConnectTimeout => settings.rdp_connect_timeout != defaults.rdp_connect_timeout,
            // The built-in list, kept or typed out, is the default either way.
            Self::RdpResolutionPresets => {
                settings.resolution_presets() != defaults.resolution_presets()
            }
            _ => false,
        }
    }

    /// The change that puts its default back, through the same message a choice on the page
    /// sends: saved and applied as that choice is. `None` for a row without a marker.
    #[must_use]
    pub fn reset(self, settings: &Settings) -> Option<SettingsMessage> {
        let defaults = Settings::default();
        if let Some(field) = self.session_field() {
            return Some(field.applied(field.value(&defaults)));
        }
        if let Some(on) = self.flag(&defaults) {
            return self.toggled(settings, on);
        }
        if let Some(path) = self.tool_path() {
            return Some(path.applied(path.value(&defaults).to_owned()));
        }
        Some(match self {
            Self::Theme => SettingsMessage::Theme(defaults.theme),
            Self::Accent => SettingsMessage::Accent(defaults.accent),
            Self::MaxSessions => SettingsMessage::MaxSessions(defaults.max_sessions),
            Self::FontSize => SettingsMessage::TerminalFontSize(defaults.terminal_font_size),
            Self::FontFamily => SettingsMessage::TerminalFontFamily(defaults.terminal_font_family),
            Self::ColorScheme => SettingsMessage::ColorScheme(defaults.color_scheme),
            Self::CtrlVPaste => SettingsMessage::CtrlVPaste(defaults.ctrl_v_paste),
            Self::PowerShellPolicy => {
                SettingsMessage::PowerShellExecutionPolicy(defaults.powershell_execution_policy)
            }
            Self::SessionLogDirectory => {
                SettingsMessage::SessionLogDirectory(defaults.session_log_directory)
            }
            Self::SshAutoReconnectAttempts => {
                SettingsMessage::SshAutoReconnectAttempts(defaults.ssh_auto_reconnect_attempts)
            }
            Self::SshAgentPreference => {
                SettingsMessage::SshAgentPreference(defaults.ssh_agent_preference)
            }
            Self::ExternalEditor => SettingsMessage::ExternalEditor(defaults.external_editor),
            Self::RdpDefaults => SettingsMessage::RdpDefaults(defaults.rdp_defaults),
            Self::RdpAutoReconnectAttempts => {
                SettingsMessage::RdpAutoReconnectAttempts(defaults.rdp_auto_reconnect_attempts)
            }
            Self::RdpConnectTimeout => {
                SettingsMessage::RdpConnectTimeout(defaults.rdp_connect_timeout)
            }
            Self::RdpResolutionPresets => {
                SettingsMessage::RdpResolutionPresets(defaults.rdp_resolution_presets)
            }
            _ => return None,
        })
    }

    /// Its value in `settings` when it is a box ticked on or off.
    #[must_use]
    pub fn flag(self, settings: &Settings) -> Option<bool> {
        let sftp = settings.sftp_browser;
        Some(match self {
            Self::CollapseTunnelsPanel => settings.collapse_tunnels_panel,
            Self::PreventSleep => settings.prevent_sleep,
            Self::Reachability => settings.reachability.enabled,
            Self::SessionLogging => settings.session_logging,
            Self::SshAutoReconnect => settings.ssh_auto_reconnect,
            Self::DisconnectOnLock => settings.disconnect_on_lock,
            Self::SftpBrowser => sftp.enabled,
            Self::SftpAutoOpen => sftp.auto_open_on_ssh,
            Self::SftpFollow => sftp.follow_ssh_directory,
            Self::DockLocalBrowser => sftp.dock_local_browser,
            Self::LocalFollow => sftp.follow_local_directory,
            Self::X11AutoStart => settings.x11_auto_start,
            _ => return None,
        })
    }

    /// Whether its box can be ticked with `settings`: the SFTP pane's boxes only with the
    /// SFTP browser on, the local browser's following only with that browser docked, as the
    /// C# checkboxes they hang from enable them.
    #[must_use]
    pub fn toggle_enabled(self, settings: &Settings) -> bool {
        let sftp = settings.sftp_browser;
        match self {
            Self::SftpAutoOpen | Self::SftpFollow => sftp.enabled,
            Self::LocalFollow => sftp.dock_local_browser,
            _ => true,
        }
    }

    /// The change its box ticked `on` sends, the other SFTP settings kept as `settings` has
    /// them; `None` when it is not a box.
    #[must_use]
    pub fn toggled(self, settings: &Settings, on: bool) -> Option<SettingsMessage> {
        let sftp = settings.sftp_browser;
        let browser = |sftp: SftpBrowser| SettingsMessage::SftpBrowser(sftp);
        Some(match self {
            Self::CollapseTunnelsPanel => SettingsMessage::CollapseTunnelsPanel(on),
            Self::PreventSleep => SettingsMessage::PreventSleep(on),
            Self::Reachability => SettingsMessage::Reachability(on),
            Self::SessionLogging => SettingsMessage::SessionLogging(on),
            Self::SshAutoReconnect => SettingsMessage::SshAutoReconnect(on),
            Self::DisconnectOnLock => SettingsMessage::DisconnectOnLock(on),
            Self::X11AutoStart => SettingsMessage::X11AutoStart(on),
            Self::SftpBrowser => browser(SftpBrowser {
                enabled: on,
                ..sftp
            }),
            Self::SftpAutoOpen => browser(SftpBrowser {
                auto_open_on_ssh: on,
                ..sftp
            }),
            Self::SftpFollow => browser(SftpBrowser {
                follow_ssh_directory: on,
                ..sftp
            }),
            Self::DockLocalBrowser => browser(SftpBrowser {
                dock_local_browser: on,
                ..sftp
            }),
            Self::LocalFollow => browser(SftpBrowser {
                follow_local_directory: on,
                ..sftp
            }),
            _ => return None,
        })
    }
}

/// A program's path typed in the Settings page and applied with Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPath {
    /// `PuTTY`.
    Putty,
    /// The X server.
    X11Server,
}

impl ToolPath {
    /// How many there are.
    pub const COUNT: usize = 2;

    /// Its place among them, for what is typed in each.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Putty => 0,
            Self::X11Server => 1,
        }
    }

    /// Its value in `settings`.
    #[must_use]
    pub fn value(self, settings: &Settings) -> &str {
        match self {
            Self::Putty => &settings.putty_path,
            Self::X11Server => &settings.x11_server_path,
        }
    }

    /// The change that sets it to `typed`.
    #[must_use]
    pub fn applied(self, typed: String) -> SettingsMessage {
        match self {
            Self::Putty => SettingsMessage::PuttyPath(typed),
            Self::X11Server => SettingsMessage::X11ServerPath(typed),
        }
    }
}

/// A line of the security overview: a security-relevant choice the application has.
///
/// The C# card has twelve; five name what this application does not have: TFTP sharing,
/// Credential Guard, Windows Hello before connecting, update checks and the `known_hosts`
/// import at startup. They are left out rather than shown in a state nothing can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostureKey {
    /// RDP Network Level Authentication.
    RdpNla,
    /// RDP strict server authentication.
    RdpStrictServerAuthentication,
    /// The session transcripts.
    SessionTranscripts,
    /// The `PowerShell` execution policy.
    PowerShellExecutionPolicy,
    /// The master password.
    Vault,
    /// The idle auto-lock.
    AutoLock,
    /// The sessions closed when the workspace locks.
    DisconnectOnLock,
}

/// The state a line of the security overview reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostureState {
    /// A choice that is on.
    On,
    /// A choice that is off.
    Off,
    /// The master password, set.
    Enabled,
    /// The master password, not set.
    Disabled,
    /// The `PowerShell` policy chosen.
    Policy(ExecutionPolicy),
    /// The workspace locks after this many minutes without input.
    AfterMinutes(u32),
    /// The workspace never locks by itself.
    Never,
    /// A workspace lock setting, which means nothing without a master password.
    RequiresVault,
}

/// A line of the security overview, as the C# decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostureLine {
    /// The choice it is about.
    pub key: PostureKey,
    /// The state of that choice.
    pub state: PostureState,
    /// The state is the documented insecure choice.
    pub risky: bool,
    /// The row "Go to setting" shows.
    pub target: SettingRow,
}

/// The security overview of `settings`, `vault` telling whether a master password is set,
/// in the C# card's order.
///
/// As the C#: NLA off is risky, as the password then goes to a server that has not proved
/// its identity; transcripts on keep everything typed; Bypass and Unrestricted turn the
/// script signing check off; a master password set with no idle lock stays unlocked for as
/// long as the application runs. Strict server authentication off is the Windows default,
/// and the master password and disconnecting on lock are hardening one opts into: their
/// states are reported, never flagged. Without a master password, the two lock lines say so
/// and lead to it, as there is nothing to lock.
#[must_use]
pub fn posture(settings: &Settings, vault: bool) -> [PostureLine; 7] {
    let on_off = |on: bool| {
        if on {
            PostureState::On
        } else {
            PostureState::Off
        }
    };
    let rdp = settings.rdp_defaults;
    let policy = settings.powershell_execution_policy;
    [
        PostureLine {
            key: PostureKey::RdpNla,
            state: on_off(rdp.nla),
            risky: !rdp.nla,
            target: SettingRow::RdpDefaults,
        },
        PostureLine {
            key: PostureKey::RdpStrictServerAuthentication,
            state: on_off(rdp.strict_server_authentication),
            risky: false,
            target: SettingRow::RdpDefaults,
        },
        PostureLine {
            key: PostureKey::SessionTranscripts,
            state: on_off(settings.session_logging),
            risky: settings.session_logging,
            target: SettingRow::SessionLogging,
        },
        PostureLine {
            key: PostureKey::PowerShellExecutionPolicy,
            state: PostureState::Policy(policy),
            risky: matches!(
                policy,
                ExecutionPolicy::Bypass | ExecutionPolicy::Unrestricted
            ),
            target: SettingRow::PowerShellPolicy,
        },
        PostureLine {
            key: PostureKey::Vault,
            state: if vault {
                PostureState::Enabled
            } else {
                PostureState::Disabled
            },
            risky: false,
            target: SettingRow::Vault,
        },
        auto_lock_line(settings.auto_lock_idle_minutes, vault),
        PostureLine {
            key: PostureKey::DisconnectOnLock,
            state: if vault {
                on_off(settings.disconnect_on_lock)
            } else {
                PostureState::RequiresVault
            },
            risky: false,
            target: if vault {
                SettingRow::DisconnectOnLock
            } else {
                SettingRow::Vault
            },
        },
    ]
}

/// The idle auto-lock's line, as the C# `AutoLock`: without a master password there is
/// nothing to lock, so it says what turns it on and leads there.
fn auto_lock_line(minutes: u32, vault: bool) -> PostureLine {
    let (state, risky, target) = if !vault {
        (PostureState::RequiresVault, false, SettingRow::Vault)
    } else if minutes == AUTO_LOCK_IDLE_MINUTES_OFF {
        (PostureState::Never, true, SettingRow::AutoLock)
    } else {
        (
            PostureState::AfterMinutes(minutes),
            false,
            SettingRow::AutoLock,
        )
    };
    PostureLine {
        key: PostureKey::AutoLock,
        state,
        risky,
        target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_card_has_rows_and_every_row_one_card() {
        for card in SettingsCard::ALL {
            assert!(!card.rows().is_empty(), "{card:?}");
        }
        let counted: usize = SettingsCard::ALL.iter().map(|card| card.rows().len()).sum();
        assert_eq!(counted, SettingRow::ALL.len());
    }

    #[test]
    fn the_defaults_are_never_modified_and_every_marked_row_resets() {
        let defaults = Settings::default();
        for row in SettingRow::ALL {
            assert!(!row.is_modified(&defaults), "{row:?}");
            assert_eq!(row.reset(&defaults).is_some(), row.is_marked(), "{row:?}");
        }
    }

    #[test]
    fn the_built_in_presets_typed_out_or_left_empty_are_the_default() {
        let mut settings = Settings {
            rdp_resolution_presets: Vec::new(),
            ..Settings::default()
        };
        assert!(!SettingRow::RdpResolutionPresets.is_modified(&settings));
        settings.rdp_resolution_presets = vec![(800, 600)];
        assert!(SettingRow::RdpResolutionPresets.is_modified(&settings));
    }

    #[test]
    fn an_sftp_box_reset_keeps_the_other_boxes() {
        let settings = Settings {
            sftp_browser: SftpBrowser {
                auto_open_on_ssh: false,
                follow_ssh_directory: true,
                ..SftpBrowser::default()
            },
            ..Settings::default()
        };
        assert_eq!(
            SettingRow::SftpAutoOpen.reset(&settings),
            Some(SettingsMessage::SftpBrowser(SftpBrowser {
                follow_ssh_directory: true,
                ..SftpBrowser::default()
            }))
        );
    }

    #[test]
    fn putty_and_the_x_server_are_on_the_ssh_tab_marked_and_reset() {
        assert_eq!(SettingsCard::Putty.rows(), [SettingRow::PuttyPath]);
        assert_eq!(
            SettingsCard::X11.rows(),
            [SettingRow::X11ServerPath, SettingRow::X11AutoStart]
        );
        for row in [
            SettingRow::PuttyPath,
            SettingRow::X11ServerPath,
            SettingRow::X11AutoStart,
        ] {
            assert_eq!(row.tab(), SettingsTab::Ssh, "{row:?}");
            assert!(row.is_marked(), "{row:?}");
        }
        let changed = Settings {
            putty_path: r"C:\Tools\putty.exe".to_owned(),
            x11_server_path: "/opt/x/vcxsrv".to_owned(),
            x11_auto_start: false,
            ..Settings::default()
        };
        for row in [
            SettingRow::PuttyPath,
            SettingRow::X11ServerPath,
            SettingRow::X11AutoStart,
        ] {
            assert!(row.is_modified(&changed), "{row:?}");
        }
        assert_eq!(
            SettingRow::PuttyPath.reset(&changed),
            Some(SettingsMessage::PuttyPath(String::new()))
        );
        assert_eq!(
            SettingRow::X11ServerPath.reset(&changed),
            Some(SettingsMessage::X11ServerPath(String::new()))
        );
        assert_eq!(
            SettingRow::X11AutoStart.reset(&changed),
            Some(SettingsMessage::X11AutoStart(true))
        );
        assert_eq!(SettingRow::X11AutoStart.flag(&changed), Some(false));
        // Spaces typed around the default are not a change.
        let padded = Settings {
            putty_path: "  ".to_owned(),
            ..Settings::default()
        };
        assert!(!SettingRow::PuttyPath.is_modified(&padded));
        assert_eq!(
            ToolPath::X11Server.applied("x".to_owned()),
            SettingsMessage::X11ServerPath("x".to_owned())
        );
        assert_ne!(ToolPath::Putty.index(), ToolPath::X11Server.index());
    }

    #[test]
    fn the_overview_flags_the_csharp_risky_choices_only() {
        let safe = posture(&Settings::default(), false);
        assert!(safe.iter().all(|line| !line.risky));
        let mut settings = Settings::default();
        settings.rdp_defaults.nla = false;
        settings.session_logging = true;
        settings.powershell_execution_policy = ExecutionPolicy::Unrestricted;
        let risky: Vec<PostureKey> = posture(&settings, true)
            .iter()
            .filter(|line| line.risky)
            .map(|line| line.key)
            .collect();
        assert_eq!(
            risky,
            [
                PostureKey::RdpNla,
                PostureKey::SessionTranscripts,
                PostureKey::PowerShellExecutionPolicy,
                PostureKey::AutoLock,
            ]
        );
        settings.powershell_execution_policy = ExecutionPolicy::RemoteSigned;
        assert!(!posture(&settings, true)[3].risky);
    }

    /// The line of `key` in the overview of `settings`, `vault` telling whether a master
    /// password is set.
    fn line(settings: &Settings, vault: bool, key: PostureKey) -> PostureLine {
        posture(settings, vault)
            .into_iter()
            .find(|line| line.key == key)
            .expect("a line")
    }

    #[test]
    fn the_lock_lines_need_the_master_password_and_only_no_idle_lock_is_risky() {
        let mut settings = Settings::default();
        for key in [PostureKey::AutoLock, PostureKey::DisconnectOnLock] {
            let without = line(&settings, false, key);
            assert_eq!(without.state, PostureState::RequiresVault, "{key:?}");
            assert!(!without.risky, "{key:?}");
            assert_eq!(
                without.target,
                SettingRow::Vault,
                "leads to what turns it on"
            );
        }
        let never = line(&settings, true, PostureKey::AutoLock);
        assert_eq!(
            (never.state, never.risky, never.target),
            (PostureState::Never, true, SettingRow::AutoLock)
        );
        settings.auto_lock_idle_minutes = 15;
        let after = line(&settings, true, PostureKey::AutoLock);
        assert_eq!(
            (after.state, after.risky),
            (PostureState::AfterMinutes(15), false)
        );
        let off = line(&settings, true, PostureKey::DisconnectOnLock);
        assert_eq!(
            (off.state, off.risky, off.target),
            (PostureState::Off, false, SettingRow::DisconnectOnLock)
        );
        settings.disconnect_on_lock = true;
        assert_eq!(
            line(&settings, true, PostureKey::DisconnectOnLock).state,
            PostureState::On
        );
    }

    #[test]
    fn the_lock_rows_are_under_the_master_password_and_need_it() {
        assert_eq!(
            SettingsCard::Vault.rows(),
            [
                SettingRow::Vault,
                SettingRow::AutoLock,
                SettingRow::DisconnectOnLock
            ]
        );
        let needing: Vec<SettingRow> = SettingRow::ALL
            .into_iter()
            .filter(|row| row.needs_vault())
            .collect();
        assert_eq!(
            needing,
            [SettingRow::AutoLock, SettingRow::DisconnectOnLock]
        );
        let changed = Settings {
            auto_lock_idle_minutes: 10,
            disconnect_on_lock: true,
            ..Settings::default()
        };
        assert!(SettingRow::AutoLock.is_modified(&changed));
        assert!(SettingRow::DisconnectOnLock.is_modified(&changed));
        assert_eq!(
            SettingRow::AutoLock.reset(&changed),
            Some(SettingsMessage::AutoLockIdleMinutes(
                AUTO_LOCK_IDLE_MINUTES_OFF
            ))
        );
        assert_eq!(
            SettingRow::DisconnectOnLock.reset(&changed),
            Some(SettingsMessage::DisconnectOnLock(false))
        );
    }
}
