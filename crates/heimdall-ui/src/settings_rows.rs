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
use heimdall_app::windows_hello;
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
    /// The session tree's view choices, as the C# "Session tree" card.
    SessionTree,
    /// The look for a newer release.
    Updates,
    /// The background check of every server.
    Reachability,
    /// The user's external tools, as the C# "External Tools" card of its Advanced tab.
    ExternalTools,
    /// The values of the `[NAME]` variables of a `MobaXterm` file, as the C# card of the `MobaXterm` import
    /// variables.
    MobaxtermVariables,
    /// The shared session sources, as the C# card of its "Shared session sources".
    SharedSources,
    /// "Show the tour again", as the C# "Welcome tour" card.
    Onboarding,
    /// The Citrix Workspace paths and store filter, as the C# Advanced tab's optional
    /// `SelfService.exe` path, cache folder and store prefix. After the tour so that the
    /// tour's card keeps its place on the page.
    Citrix,
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
    /// `PuTTY`, which SSH profiles in the external mode open in, and the default SSH mode.
    Putty,
    /// The SFTP browser and the local file browser.
    Sftp,
    /// The X server X11 forwarding draws on.
    X11,
    /// The program a server's file is edited with.
    ExternalEditor,
    /// The trusted SSH host keys and FTPS certificates.
    SshTrusted,
    /// The default RDP mode and the RDP options profiles following the application's take.
    RdpDefaults,
    /// The RDP auto-reconnect attempts, the logon watchdog, the wait after connecting and
    /// the keep-alive interval.
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
    /// What a connection passes first: Credential Guard, then Windows Hello.
    ConnectionChecks,
}

impl SettingsCard {
    /// Every card, in the page's order.
    pub const ALL: [Self; 30] = [
        Self::Appearance,
        Self::Behavior,
        Self::SessionTree,
        Self::Updates,
        Self::Reachability,
        Self::ExternalTools,
        Self::MobaxtermVariables,
        Self::SharedSources,
        Self::Onboarding,
        Self::Citrix,
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
        Self::ConnectionChecks,
    ];

    /// The tab it is on.
    #[must_use]
    pub fn tab(self) -> SettingsTab {
        match self {
            Self::Appearance
            | Self::Behavior
            | Self::SessionTree
            | Self::Updates
            | Self::Reachability
            // The C# keeps them on its Advanced tab, which this version does not have.
            | Self::ExternalTools
            | Self::MobaxtermVariables
            | Self::SharedSources
            | Self::Onboarding
            | Self::Citrix => SettingsTab::General,
            Self::Terminal | Self::SessionLogging | Self::Macros => SettingsTab::Terminal,
            Self::SshReconnect
            | Self::SshSession
            | Self::Putty
            | Self::Sftp
            | Self::X11
            | Self::ExternalEditor
            | Self::SshTrusted
            | Self::Gateways => SettingsTab::Ssh,
            Self::RdpDefaults
            | Self::RdpSession
            | Self::RdpPresets
            | Self::RdpReset
            | Self::RdpTrusted => SettingsTab::Rdp,
            Self::Pin | Self::Vault | Self::Provider | Self::ConnectionChecks => {
                SettingsTab::Security
            }
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

    /// The section of the rail it is shown in; none for a card drawn under every section of
    /// its group, as the C# "Reset RDP defaults" under the RDP sections.
    #[must_use]
    pub fn section(self) -> Option<SettingsSection> {
        Some(match self {
            Self::Appearance | Self::SessionTree => SettingsSection::Appearance,
            // The C# keeps the background check on its Advanced tab, which this version
            // does not have: it is a behaviour of the window.
            Self::Behavior | Self::Reachability => SettingsSection::Behavior,
            Self::Updates => SettingsSection::Updates,
            Self::MobaxtermVariables => SettingsSection::Import,
            Self::SharedSources => SettingsSection::SharedSources,
            Self::ExternalTools | Self::Citrix => SettingsSection::Tools,
            Self::Onboarding => SettingsSection::Onboarding,
            // The C# gives Terminal one section for its single card.
            Self::Terminal | Self::SessionLogging | Self::Macros => SettingsSection::Terminal,
            Self::Putty => SettingsSection::SshConnection,
            Self::SshReconnect | Self::SshSession => SettingsSection::SshSession,
            // The editor opens a server's file over SFTP; the C# keeps it on Advanced.
            Self::Sftp | Self::X11 | Self::ExternalEditor => SettingsSection::SftpX11,
            Self::SshTrusted => SettingsSection::SshHostKeys,
            Self::RdpDefaults => SettingsSection::RdpDisplay,
            Self::RdpSession | Self::RdpPresets => SettingsSection::RdpBehavior,
            Self::RdpTrusted => SettingsSection::RdpCertificates,
            Self::Gateways => SettingsSection::Gateways,
            Self::Pin | Self::Vault => SettingsSection::Vault,
            Self::Provider => SettingsSection::Provider,
            Self::ConnectionChecks => SettingsSection::ConnectionChecks,
            Self::RdpReset => return None,
        })
    }
}

/// When a setting takes effect, if not as soon as it is changed. Every setting applies and is
/// saved as it is changed; these few are read when a session or connection is opened, so the
/// ones already open keep the old value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// On the connections opened after the change.
    NewConnections,
    /// On the sessions opened after the change.
    NewSessions,
}

impl SettingRow {
    /// Whether it is read when a session or connection opens, checked in the code that reads
    /// it: the `PowerShell` policy when a local shell starts, the agent preference, the SSH
    /// keep-alive and the `PuTTY` path when a connection or launch is made, the RDP logon
    /// timeout and keep-alive when an RDP session opens. Everything else, the auto-reconnect
    /// attempts and delays, the terminal's font, size, keys and colours, applies at once.
    #[must_use]
    pub fn applies(self) -> Option<Applies> {
        match self {
            Self::PowerShellPolicy
            | Self::SessionLogging
            | Self::SftpAutoOpen
            | Self::DockLocalBrowser => Some(Applies::NewSessions),
            Self::SshAgentPreference
            | Self::KeepAlive
            | Self::PuttyPath
            | Self::RdpConnectTimeout
            | Self::RdpKeepAlive
            | Self::RdpBuiltinCompression => Some(Applies::NewConnections),
            _ => None,
        }
    }
}

/// A section of the Settings rail, as the C# `SettingsRail` items: the cards of a group,
/// split so each is read without scrolling a whole tab. One rail lists every section under
/// the name of its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    /// The language, theme and accent, and the session tree's view.
    Appearance,
    /// The tunnels panel, the computer's sleep, the most sessions, the background check.
    Behavior,
    /// The look for a newer release.
    Updates,
    /// The values of a `MobaXterm` file's variables.
    Import,
    /// The shared session sources.
    SharedSources,
    /// The user's external tools, as the C# "Tools & integrations".
    Tools,
    /// "Show the tour again", last as the C# "Welcome tour".
    Onboarding,
    /// The terminals' text and colours, the transcripts and the macros.
    Terminal,
    /// `PuTTY` and the default SSH mode, as the C# SSH "Connection".
    SshConnection,
    /// Auto-reconnect and the session's numbers, as the C# SSH "Session".
    SshSession,
    /// The SFTP browser, the X server and the external editor, as the C# "SFTP & X11".
    SftpX11,
    /// The trusted host keys and certificates.
    SshHostKeys,
    /// The RDP options profiles take, as the C# "Display & Audio".
    RdpDisplay,
    /// Reconnecting, waiting, and the sizes offered, as the C# RDP "Behavior".
    RdpBehavior,
    /// The trusted RDP certificates.
    RdpCertificates,
    /// The SSH gateways.
    Gateways,
    /// The security overview, first as the C# "Overview".
    Overview,
    /// The PIN and the master password.
    Vault,
    /// The external credential provider.
    Provider,
    /// What a connection passes first.
    ConnectionChecks,
}

impl SettingsSection {
    /// Every section, in the rail's order: group by group, in the order the tabs had.
    pub const ALL: [Self; 20] = [
        Self::Appearance,
        Self::Behavior,
        Self::Updates,
        Self::Import,
        Self::SharedSources,
        Self::Tools,
        Self::Onboarding,
        Self::Terminal,
        Self::SshConnection,
        Self::SshSession,
        Self::SftpX11,
        Self::SshHostKeys,
        Self::Gateways,
        Self::RdpDisplay,
        Self::RdpBehavior,
        Self::RdpCertificates,
        Self::Overview,
        Self::Vault,
        Self::Provider,
        Self::ConnectionChecks,
    ];

    /// The group whose name heads it on the rail.
    #[must_use]
    pub const fn tab(self) -> SettingsTab {
        match self {
            Self::Appearance
            | Self::Behavior
            | Self::Updates
            | Self::Import
            | Self::SharedSources
            | Self::Tools
            | Self::Onboarding => SettingsTab::General,
            Self::Terminal => SettingsTab::Terminal,
            Self::SshConnection
            | Self::SshSession
            | Self::SftpX11
            | Self::SshHostKeys
            | Self::Gateways => SettingsTab::Ssh,
            Self::RdpDisplay | Self::RdpBehavior | Self::RdpCertificates => SettingsTab::Rdp,
            Self::Overview | Self::Vault | Self::Provider | Self::ConnectionChecks => {
                SettingsTab::Security
            }
        }
    }

    /// The sections of `tab`, in the rail's order.
    #[must_use]
    pub fn of_tab(tab: SettingsTab) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|section| section.tab() == tab)
            .collect()
    }

    /// The first section of `tab`: the one a name of the old tab strip opens.
    #[must_use]
    pub fn first_of(tab: SettingsTab) -> Self {
        Self::ALL
            .into_iter()
            .find(|section| section.tab() == tab)
            .unwrap_or(Self::Appearance)
    }

    /// Its cards, in the page's order.
    #[must_use]
    pub fn cards(self) -> Vec<SettingsCard> {
        SettingsCard::ALL
            .into_iter()
            .filter(|card| card.section() == Some(self))
            .collect()
    }

    /// The section `steps` after it down the rail, across the groups' names, cycling round
    /// as the C# rail's `DirectionalNavigation="Cycle"`; `steps` may be negative.
    #[must_use]
    pub fn moved(self, steps: isize) -> Self {
        let count = Self::ALL.len().cast_signed();
        let index = Self::ALL
            .iter()
            .position(|section| *section == self)
            .map_or(0, usize::cast_signed);
        Self::ALL[(index + steps).rem_euclid(count).cast_unsigned()]
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
    /// The properties panel docked under the tree, shared with the tree's filter menu.
    ShowPropertiesPanel,
    /// The computer kept from sleeping while a session is open.
    PreventSleep,
    /// Most sessions open at once.
    MaxSessions,
    /// A newer release is looked for by itself.
    UpdateChecks,
    /// Hours between two looks.
    UpdateInterval,
    /// The version running, with "Check now" and what it found.
    UpdateVersion,
    /// "Offer legacy migration at next startup", once an offer was declined.
    LegacyMigration,
    /// The background check of every server runs.
    Reachability,
    /// Seconds between two background checks.
    ReachabilityInterval,
    /// Milliseconds a server has to answer one.
    ReachabilityTimeout,
    /// Servers checked at once.
    ReachabilityProbes,
    /// The external tools, listed and edited.
    ExternalTools,
    /// The `MobaXterm` import variables, listed and edited.
    MobaxtermVariables,
    /// The shared session sources, listed and edited.
    SharedSources,
    /// "Show the tour again", which shows the welcome tour from its first step.
    Onboarding,
    /// The `SelfService.exe` Citrix cache launches go through.
    CitrixSelfServicePath,
    /// The folder the Citrix Workspace cache is read from.
    CitrixCacheDirectory,
    /// The store file-name prefix of the Citrix cache files read.
    CitrixCacheStorePrefix,
    /// The size of the terminals' text.
    FontSize,
    /// The family of the terminals' text.
    FontFamily,
    /// The terminals' colours.
    ColorScheme,
    /// How much contrast terminal text keeps against its background.
    TerminalMinimumContrast,
    /// What Ctrl+V does in a terminal.
    CtrlVPaste,
    /// What Ctrl+K does in a terminal.
    CtrlKTerminal,
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
    /// Seconds before the first SSH auto-reconnect attempt.
    SshAutoReconnectFirstDelay,
    /// Seconds before the second SSH auto-reconnect attempt.
    SshAutoReconnectSecondDelay,
    /// Seconds before every SSH auto-reconnect attempt after the second.
    SshAutoReconnectSubsequentDelay,
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
    /// Where the shell of a new SSH profile opens, with "Apply to all saved sessions".
    SshDefaultMode,
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
    /// The user's OpenSSH `known_hosts` imported at each start.
    SyncKnownHostsAtStartup,
    /// The trusted FTPS certificates.
    FtpsCertificates,
    /// The trusted VNC certificates.
    VncCertificates,
    /// Where the desktop of a new RDP profile opens, with "Apply to all saved sessions".
    RdpDefaultMode,
    /// The RDP options profiles following the application's take.
    RdpDefaults,
    /// Attempts of an RDP auto-reconnect.
    RdpAutoReconnectAttempts,
    /// Seconds an RDP connection may take to log on.
    RdpConnectTimeout,
    /// Milliseconds an RDP desktop following its tab waits after connecting.
    RdpResizeDelay,
    /// Milliseconds between two keep-alives of an embedded RDP connection.
    RdpKeepAlive,
    /// Pixels of the width Remote Desktop Connection opens at for the default resolution.
    RdpDefaultWidth,
    /// Pixels of the height Remote Desktop Connection opens at for the default resolution.
    RdpDefaultHeight,
    /// The built-in RDP client compresses when the profile asks for it, experimental.
    RdpBuiltinCompression,
    /// The session bar's Disconnect asks before it ends an RDP desktop.
    RdpConfirmDisconnect,
    /// An RDP desktop's bar shows every redirection, the ones not shared too.
    RdpRedirectionIndicators,
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
    /// An embedded RDP session opens only while Credential Guard runs.
    RequireCredentialGuard,
    /// A connection waits for Windows Hello.
    RequireWindowsHello,
    /// Minutes a Windows Hello verification counts.
    WindowsHelloGrace,
    /// Days Windows Hello unlocks the vault before the master password is asked again.
    VaultHelloMaxDays,
}

impl SettingRow {
    /// Every row, in the page's order.
    pub const ALL: [Self; 80] = [
        Self::Language,
        Self::Theme,
        Self::Accent,
        Self::CollapseTunnelsPanel,
        Self::PreventSleep,
        Self::MaxSessions,
        Self::ShowPropertiesPanel,
        Self::UpdateChecks,
        Self::UpdateInterval,
        Self::UpdateVersion,
        Self::LegacyMigration,
        Self::Reachability,
        Self::ReachabilityInterval,
        Self::ReachabilityTimeout,
        Self::ReachabilityProbes,
        Self::ExternalTools,
        Self::MobaxtermVariables,
        Self::SharedSources,
        Self::Onboarding,
        Self::CitrixSelfServicePath,
        Self::CitrixCacheDirectory,
        Self::CitrixCacheStorePrefix,
        Self::FontSize,
        Self::FontFamily,
        Self::ColorScheme,
        Self::TerminalMinimumContrast,
        Self::CtrlVPaste,
        Self::CtrlKTerminal,
        Self::PowerShellPolicy,
        Self::SessionLogging,
        Self::SessionLogDirectory,
        Self::TranscriptRetention,
        Self::Macros,
        Self::SshAutoReconnect,
        Self::SshAutoReconnectAttempts,
        Self::SshAutoReconnectFirstDelay,
        Self::SshAutoReconnectSecondDelay,
        Self::SshAutoReconnectSubsequentDelay,
        Self::SshAgentPreference,
        Self::KeepAlive,
        Self::TmoutReset,
        Self::AntiIdle,
        Self::PuttyPath,
        Self::SshDefaultMode,
        Self::SftpBrowser,
        Self::SftpAutoOpen,
        Self::SftpFollow,
        Self::DockLocalBrowser,
        Self::LocalFollow,
        Self::X11ServerPath,
        Self::X11AutoStart,
        Self::ExternalEditor,
        Self::HostKeys,
        Self::SyncKnownHostsAtStartup,
        Self::FtpsCertificates,
        Self::VncCertificates,
        Self::RdpDefaultMode,
        Self::RdpDefaults,
        Self::RdpAutoReconnectAttempts,
        Self::RdpConnectTimeout,
        Self::RdpResizeDelay,
        Self::RdpKeepAlive,
        Self::RdpConfirmDisconnect,
        Self::RdpRedirectionIndicators,
        Self::RdpDefaultWidth,
        Self::RdpDefaultHeight,
        Self::RdpBuiltinCompression,
        Self::RdpResolutionPresets,
        Self::RdpResetAll,
        Self::Certificates,
        Self::Gateways,
        Self::Pin,
        Self::Vault,
        Self::VaultHelloMaxDays,
        Self::AutoLock,
        Self::DisconnectOnLock,
        Self::Provider,
        Self::RequireCredentialGuard,
        Self::RequireWindowsHello,
        Self::WindowsHelloGrace,
    ];

    /// The card it is on.
    #[must_use]
    pub fn card(self) -> SettingsCard {
        match self {
            Self::Language | Self::Theme | Self::Accent => SettingsCard::Appearance,
            Self::CollapseTunnelsPanel | Self::PreventSleep | Self::MaxSessions => {
                SettingsCard::Behavior
            }
            Self::ShowPropertiesPanel => SettingsCard::SessionTree,
            Self::UpdateChecks
            | Self::UpdateInterval
            | Self::UpdateVersion
            | Self::LegacyMigration => SettingsCard::Updates,
            Self::Reachability
            | Self::ReachabilityInterval
            | Self::ReachabilityTimeout
            | Self::ReachabilityProbes => SettingsCard::Reachability,
            Self::ExternalTools => SettingsCard::ExternalTools,
            Self::MobaxtermVariables => SettingsCard::MobaxtermVariables,
            Self::SharedSources => SettingsCard::SharedSources,
            Self::CitrixSelfServicePath
            | Self::CitrixCacheDirectory
            | Self::CitrixCacheStorePrefix => SettingsCard::Citrix,
            // The last card of the C# General tab (`MainWindow.xaml:2715-2745`).
            Self::Onboarding => SettingsCard::Onboarding,
            Self::FontSize
            | Self::FontFamily
            | Self::ColorScheme
            | Self::TerminalMinimumContrast
            | Self::CtrlVPaste
            | Self::CtrlKTerminal
            | Self::PowerShellPolicy => SettingsCard::Terminal,
            Self::SessionLogging | Self::SessionLogDirectory | Self::TranscriptRetention => {
                SettingsCard::SessionLogging
            }
            Self::Macros => SettingsCard::Macros,
            Self::SshAutoReconnect
            | Self::SshAutoReconnectAttempts
            | Self::SshAutoReconnectFirstDelay
            | Self::SshAutoReconnectSecondDelay
            | Self::SshAutoReconnectSubsequentDelay
            | Self::SshAgentPreference => SettingsCard::SshReconnect,
            Self::KeepAlive | Self::TmoutReset | Self::AntiIdle => SettingsCard::SshSession,
            Self::PuttyPath | Self::SshDefaultMode => SettingsCard::Putty,
            Self::SftpBrowser
            | Self::SftpAutoOpen
            | Self::SftpFollow
            | Self::DockLocalBrowser
            | Self::LocalFollow => SettingsCard::Sftp,
            Self::X11ServerPath | Self::X11AutoStart => SettingsCard::X11,
            Self::ExternalEditor => SettingsCard::ExternalEditor,
            // The import at startup under the host keys it adds to, as the C# box under
            // their buttons.
            Self::HostKeys
            | Self::SyncKnownHostsAtStartup
            | Self::FtpsCertificates
            | Self::VncCertificates => SettingsCard::SshTrusted,
            // At the top of the C# "RDP defaults" card (`MainWindow.xaml:3356-3375`).
            Self::RdpDefaultMode | Self::RdpDefaults => SettingsCard::RdpDefaults,
            // The keep-alive last of the C# RDP Behavior card, then the built-in client's
            // compression, which the C# has no switch for.
            Self::RdpAutoReconnectAttempts
            | Self::RdpConnectTimeout
            | Self::RdpResizeDelay
            | Self::RdpKeepAlive
            | Self::RdpBuiltinCompression
            | Self::RdpConfirmDisconnect
            | Self::RdpDefaultWidth
            | Self::RdpDefaultHeight
            | Self::RdpRedirectionIndicators => SettingsCard::RdpSession,
            Self::RdpResolutionPresets => SettingsCard::RdpPresets,
            Self::RdpResetAll => SettingsCard::RdpReset,
            Self::Certificates => SettingsCard::RdpTrusted,
            Self::Gateways => SettingsCard::Gateways,
            Self::Pin => SettingsCard::Pin,
            // Under the master password, as the C# `SettingsSectionVault` holds them.
            Self::Vault | Self::VaultHelloMaxDays | Self::AutoLock | Self::DisconnectOnLock => {
                SettingsCard::Vault
            }
            Self::Provider => SettingsCard::Provider,
            // As the C# `SettingsSectionConnectionChecks`: Credential Guard, then Windows
            // Hello.
            Self::RequireCredentialGuard | Self::RequireWindowsHello | Self::WindowsHelloGrace => {
                SettingsCard::ConnectionChecks
            }
        }
    }

    /// The tab it is on.
    #[must_use]
    pub fn tab(self) -> SettingsTab {
        self.card().tab()
    }

    /// The number it is, when it is one typed and applied once in its range.
    #[must_use]
    pub fn session_field(self) -> Option<SessionField> {
        Some(match self {
            Self::ReachabilityInterval => SessionField::ReachabilityInterval,
            Self::ReachabilityTimeout => SessionField::ReachabilityTimeout,
            Self::ReachabilityProbes => SessionField::ReachabilityProbes,
            Self::TranscriptRetention => SessionField::TranscriptRetention,
            Self::SshAutoReconnectFirstDelay => SessionField::SshReconnectFirstDelay,
            Self::SshAutoReconnectSecondDelay => SessionField::SshReconnectSecondDelay,
            Self::SshAutoReconnectSubsequentDelay => SessionField::SshReconnectSubsequentDelay,
            Self::KeepAlive => SessionField::KeepAlive,
            Self::TmoutReset => SessionField::TmoutReset,
            Self::AntiIdle => SessionField::AntiIdle,
            Self::AutoLock => SessionField::AutoLock,
            Self::UpdateInterval => SessionField::UpdateInterval,
            Self::WindowsHelloGrace => SessionField::WindowsHelloGrace,
            Self::VaultHelloMaxDays => SessionField::VaultHelloMaxDays,
            Self::RdpResizeDelay => SessionField::RdpResizeDelay,
            Self::RdpKeepAlive => SessionField::RdpKeepAlive,
            Self::RdpDefaultWidth => SessionField::RdpDefaultWidth,
            Self::RdpDefaultHeight => SessionField::RdpDefaultHeight,
            _ => return None,
        })
    }

    /// The program's path it is, when it is one typed and applied with Enter.
    #[must_use]
    pub fn tool_path(self) -> Option<ToolPath> {
        match self {
            Self::PuttyPath => Some(ToolPath::Putty),
            Self::X11ServerPath => Some(ToolPath::X11Server),
            Self::CitrixSelfServicePath => Some(ToolPath::CitrixSelfService),
            Self::CitrixCacheDirectory => Some(ToolPath::CitrixCacheDirectory),
            Self::CitrixCacheStorePrefix => Some(ToolPath::CitrixStorePrefix),
            _ => None,
        }
    }

    /// Whether it means something only with a master password set: the workspace lock's
    /// settings and Windows Hello's days, shown disabled without one as the C# shows them.
    #[must_use]
    pub fn needs_vault(self) -> bool {
        matches!(
            self,
            Self::AutoLock | Self::DisconnectOnLock | Self::VaultHelloMaxDays
        )
    }

    /// Whether its number can be typed with `settings`: the Windows Hello grace only with
    /// Windows Hello required, as the C# panel enables it.
    #[must_use]
    pub fn number_enabled(self, settings: &Settings) -> bool {
        match self {
            Self::WindowsHelloGrace => settings.windows_hello.require_on_connect,
            _ => true,
        }
    }

    /// Whether it carries a "Modified" marker and a reset: a value with a default. The
    /// language follows the desktop unless chosen, which the list cannot offer back, and the
    /// C# keeps it apart from its settings; the lists and cards are inventories (keys,
    /// macros, gateways) or secrets (the PIN, the vault, the provider's secret), as the C#
    /// leaves its external tools and its unlock secret unmarked. The version running is no
    /// choice.
    #[must_use]
    pub fn is_marked(self) -> bool {
        !matches!(
            self,
            Self::Language
                | Self::UpdateVersion
                | Self::LegacyMigration
                | Self::Onboarding
                | Self::ExternalTools
                | Self::MobaxtermVariables
                | Self::SharedSources
                | Self::Macros
                | Self::HostKeys
                | Self::FtpsCertificates
                | Self::VncCertificates
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
            Self::TerminalMinimumContrast => {
                settings.terminal_minimum_contrast != defaults.terminal_minimum_contrast
            }
            Self::CtrlVPaste => settings.ctrl_v_paste != defaults.ctrl_v_paste,
            Self::CtrlKTerminal => settings.ctrl_k_terminal != defaults.ctrl_k_terminal,
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
            Self::SshDefaultMode => settings.ssh_default_mode != defaults.ssh_default_mode,
            Self::ExternalEditor => settings.external_editor.trim() != defaults.external_editor,
            Self::RdpDefaultMode => settings.rdp_default_mode != defaults.rdp_default_mode,
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
            Self::TerminalMinimumContrast => {
                SettingsMessage::TerminalMinimumContrast(defaults.terminal_minimum_contrast)
            }
            Self::CtrlVPaste => SettingsMessage::CtrlVPaste(defaults.ctrl_v_paste),
            Self::CtrlKTerminal => SettingsMessage::CtrlKTerminal(defaults.ctrl_k_terminal),
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
            Self::SshDefaultMode => SettingsMessage::SshDefaultMode(defaults.ssh_default_mode),
            Self::ExternalEditor => SettingsMessage::ExternalEditor(defaults.external_editor),
            Self::RdpDefaultMode => SettingsMessage::RdpDefaultMode(defaults.rdp_default_mode),
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
            Self::ShowPropertiesPanel => settings.show_properties_panel,
            Self::PreventSleep => settings.prevent_sleep,
            Self::UpdateChecks => settings.updates.enabled,
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
            Self::SyncKnownHostsAtStartup => settings.sync_known_hosts_at_startup,
            Self::RequireCredentialGuard => settings.require_credential_guard,
            Self::RequireWindowsHello => settings.windows_hello.require_on_connect,
            Self::RdpBuiltinCompression => settings.builtin_rdp_compression,
            Self::RdpConfirmDisconnect => settings.rdp_confirm_disconnect,
            Self::RdpRedirectionIndicators => settings.rdp_redirection_indicators_expanded,
            _ => return None,
        })
    }

    /// Whether its box can be ticked with `settings`: the SFTP pane's boxes only with the
    /// SFTP browser on, the local browser's following only with that browser docked, as the
    /// C# checkboxes they hang from enable them. Windows Hello only where it exists; ticked
    /// elsewhere (a settings file carried over), it can still be cleared.
    #[must_use]
    pub fn toggle_enabled(self, settings: &Settings) -> bool {
        let sftp = settings.sftp_browser;
        match self {
            Self::SftpAutoOpen | Self::SftpFollow => sftp.enabled,
            Self::LocalFollow => sftp.dock_local_browser,
            Self::RequireWindowsHello => {
                windows_hello::SUPPORTED || settings.windows_hello.require_on_connect
            }
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
            Self::ShowPropertiesPanel => SettingsMessage::ShowPropertiesPanel(on),
            Self::PreventSleep => SettingsMessage::PreventSleep(on),
            Self::UpdateChecks => SettingsMessage::UpdateChecks(on),
            Self::Reachability => SettingsMessage::Reachability(on),
            Self::SessionLogging => SettingsMessage::SessionLogging(on),
            Self::SshAutoReconnect => SettingsMessage::SshAutoReconnect(on),
            Self::DisconnectOnLock => SettingsMessage::DisconnectOnLock(on),
            Self::X11AutoStart => SettingsMessage::X11AutoStart(on),
            Self::SyncKnownHostsAtStartup => SettingsMessage::SyncKnownHostsAtStartup(on),
            Self::RequireCredentialGuard => SettingsMessage::RequireCredentialGuard(on),
            Self::RequireWindowsHello => SettingsMessage::RequireWindowsHello(on),
            Self::RdpBuiltinCompression => SettingsMessage::RdpBuiltinCompression(on),
            Self::RdpConfirmDisconnect => SettingsMessage::RdpConfirmDisconnect(on),
            Self::RdpRedirectionIndicators => SettingsMessage::RdpRedirectionIndicatorsExpanded(on),
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
    /// The `SelfService.exe` of Citrix Workspace.
    CitrixSelfService,
    /// The Citrix Workspace cache folder.
    CitrixCacheDirectory,
    /// The Citrix store file-name prefix: typed like a path, but with nothing to browse.
    CitrixStorePrefix,
}

impl ToolPath {
    /// How many there are.
    pub const COUNT: usize = 5;

    /// Its place among them, for what is typed in each.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Putty => 0,
            Self::X11Server => 1,
            Self::CitrixSelfService => 2,
            Self::CitrixCacheDirectory => 3,
            Self::CitrixStorePrefix => 4,
        }
    }

    /// Its value in `settings`.
    #[must_use]
    pub fn value(self, settings: &Settings) -> &str {
        match self {
            Self::Putty => &settings.putty_path,
            Self::X11Server => &settings.x11_server_path,
            Self::CitrixSelfService => &settings.citrix_self_service_path,
            Self::CitrixCacheDirectory => &settings.citrix_cache_directory,
            Self::CitrixStorePrefix => &settings.citrix_cache_store_prefix,
        }
    }

    /// The change that sets it to `typed`.
    #[must_use]
    pub fn applied(self, typed: String) -> SettingsMessage {
        match self {
            Self::Putty => SettingsMessage::PuttyPath(typed),
            Self::X11Server => SettingsMessage::X11ServerPath(typed),
            Self::CitrixSelfService => SettingsMessage::CitrixSelfServicePath(typed),
            Self::CitrixCacheDirectory => SettingsMessage::CitrixCacheDirectory(typed),
            Self::CitrixStorePrefix => SettingsMessage::CitrixCacheStorePrefix(typed),
        }
    }

    /// Whether the Settings accept `text` for it: blank always, a Citrix path or prefix only
    /// when it passes its check. The page says so when it does not, and keeps the old value.
    #[must_use]
    pub fn accepts(self, text: &str) -> bool {
        use heimdall_core::import::citrix_cache as citrix;
        let text = text.trim();
        text.is_empty()
            || match self {
                Self::CitrixSelfService => citrix::self_service_path_valid(text),
                Self::CitrixCacheDirectory => citrix::cache_directory_valid(text),
                Self::CitrixStorePrefix => citrix::normalize_store_prefix(text).is_some(),
                Self::Putty | Self::X11Server => true,
            }
    }

    /// Whether a "Browse..." button picks it: not the store prefix, which is no path.
    #[must_use]
    pub fn browsable(self) -> bool {
        !matches!(self, Self::CitrixStorePrefix)
    }
}

/// A line of the security overview: a security-relevant choice the application has.
///
/// The C# card has twelve; one names what this application does not have: TFTP sharing. It
/// is left out rather than shown in a state nothing can change.
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
    /// Credential Guard required before an embedded RDP session.
    CredentialGuard,
    /// Windows Hello asked before connecting.
    WindowsHelloOnConnect,
    /// The automatic look for a newer release.
    UpdateChecks,
    /// The user's OpenSSH `known_hosts` imported at each start.
    KnownHostsSync,
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
    /// A check a connection passes first, asked.
    Required,
    /// A check a connection passes first, not asked.
    NotRequired,
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
/// and the master password, disconnecting on lock, Credential Guard and Windows Hello before
/// connecting are hardening one opts into: their states are reported, never flagged. Update checks off are
/// risky: a security release goes unnoticed. The `known_hosts` import at startup on is
/// risky: keys a file other programs write are trusted without asking. Without a master
/// password, the two lock lines say so and lead to it, as there is nothing to lock.
#[must_use]
pub fn posture(settings: &Settings, vault: bool) -> [PostureLine; 11] {
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
        PostureLine {
            key: PostureKey::CredentialGuard,
            state: if settings.require_credential_guard {
                PostureState::Required
            } else {
                PostureState::NotRequired
            },
            risky: false,
            target: SettingRow::RequireCredentialGuard,
        },
        PostureLine {
            key: PostureKey::WindowsHelloOnConnect,
            state: if settings.windows_hello.require_on_connect {
                PostureState::Required
            } else {
                PostureState::NotRequired
            },
            risky: false,
            target: SettingRow::RequireWindowsHello,
        },
        PostureLine {
            key: PostureKey::UpdateChecks,
            state: on_off(settings.updates.enabled),
            risky: !settings.updates.enabled,
            target: SettingRow::UpdateChecks,
        },
        PostureLine {
            key: PostureKey::KnownHostsSync,
            state: on_off(settings.sync_known_hosts_at_startup),
            risky: settings.sync_known_hosts_at_startup,
            target: SettingRow::SyncKnownHostsAtStartup,
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
    use heimdall_core::profile::{RdpMode, SshMode};

    use super::*;

    #[test]
    fn the_rdp_confirmation_and_indicators_are_boxes_and_the_ssh_waits_are_numbers() {
        let defaults = Settings::default();
        assert_eq!(SettingRow::RdpConfirmDisconnect.flag(&defaults), Some(true));
        assert_eq!(
            SettingRow::RdpRedirectionIndicators.flag(&defaults),
            Some(false)
        );
        let changed = Settings {
            rdp_confirm_disconnect: false,
            rdp_redirection_indicators_expanded: true,
            ..Settings::default()
        };
        assert!(SettingRow::RdpConfirmDisconnect.is_modified(&changed));
        assert_eq!(
            SettingRow::RdpConfirmDisconnect.toggled(&defaults, false),
            Some(SettingsMessage::RdpConfirmDisconnect(false))
        );
        assert_eq!(
            SettingRow::RdpRedirectionIndicators.reset(&changed),
            Some(SettingsMessage::RdpRedirectionIndicatorsExpanded(false))
        );
        for (row, field) in [
            (
                SettingRow::SshAutoReconnectFirstDelay,
                SessionField::SshReconnectFirstDelay,
            ),
            (
                SettingRow::SshAutoReconnectSecondDelay,
                SessionField::SshReconnectSecondDelay,
            ),
            (
                SettingRow::SshAutoReconnectSubsequentDelay,
                SessionField::SshReconnectSubsequentDelay,
            ),
        ] {
            assert_eq!(row.session_field(), Some(field));
            assert_eq!(row.tab(), SettingsTab::Ssh);
            assert!(field.accepted(1) && field.accepted(600));
            assert!(!field.accepted(0) && !field.accepted(601));
            assert!(!row.is_modified(&defaults));
        }
        let waits = Settings {
            ssh_auto_reconnect_second_delay: 30,
            ..Settings::default()
        };
        assert!(SettingRow::SshAutoReconnectSecondDelay.is_modified(&waits));
        assert_eq!(
            SessionField::SshReconnectSecondDelay.applied(30),
            SettingsMessage::SshAutoReconnectSecondDelay(30)
        );
    }

    #[test]
    fn the_built_in_client_s_compression_is_a_box_off_by_default_last_of_the_rdp_session_card() {
        let row = SettingRow::RdpBuiltinCompression;
        assert_eq!(row.tab(), SettingsTab::Rdp);
        assert_eq!(
            SettingsCard::RdpSession.rows().last(),
            Some(&row),
            "after the C# card's keep-alive"
        );
        assert!(row.is_marked());
        let defaults = Settings::default();
        assert_eq!(row.flag(&defaults), Some(true), "on by default (#442)");
        assert!(row.toggle_enabled(&defaults));
        assert!(!row.is_modified(&defaults));
        let changed = Settings {
            builtin_rdp_compression: false,
            ..Settings::default()
        };
        assert!(row.is_modified(&changed));
        assert_eq!(
            row.reset(&changed),
            Some(SettingsMessage::RdpBuiltinCompression(true))
        );
        assert_eq!(
            row.toggled(&defaults, true),
            Some(SettingsMessage::RdpBuiltinCompression(true))
        );
    }

    #[test]
    fn every_section_is_listed_once_and_every_card_but_the_reset_is_in_one() {
        for (index, section) in SettingsSection::ALL.into_iter().enumerate() {
            assert!(
                !SettingsSection::ALL[..index].contains(&section),
                "{section:?} twice"
            );
        }
        for card in SettingsCard::ALL {
            match card.section() {
                Some(section) => {
                    assert_eq!(section.tab(), card.tab(), "{card:?}");
                    assert!(section.cards().contains(&card), "{card:?}");
                }
                // Drawn under every section of its group, as the C# reset under the RDP
                // sections.
                None => assert_eq!(card, SettingsCard::RdpReset),
            }
        }
        // A section with no card is the overview, drawn from the settings and not from rows.
        for section in SettingsSection::ALL {
            assert_eq!(
                section.cards().is_empty(),
                section == SettingsSection::Overview,
                "{section:?}"
            );
        }
    }

    #[test]
    fn the_rail_lists_the_old_tabs_sections_group_by_group_in_the_old_tab_order() {
        use SettingsSection as S;
        let groups: Vec<SettingsTab> = SettingsSection::ALL
            .into_iter()
            .map(SettingsSection::tab)
            .fold(Vec::new(), |mut groups, tab| {
                // A group is one run of the rail, never split in two.
                assert!(
                    groups.last() == Some(&tab) || !groups.contains(&tab),
                    "{tab:?} split"
                );
                if groups.last() != Some(&tab) {
                    groups.push(tab);
                }
                groups
            });
        assert_eq!(groups, SettingsTab::ALL);
        assert_eq!(
            SettingsSection::of_tab(SettingsTab::General),
            [
                S::Appearance,
                S::Behavior,
                S::Updates,
                S::Import,
                S::SharedSources,
                S::Tools,
                S::Onboarding
            ],
            "Appearance first, the tour last, as the C# rail"
        );
        assert_eq!(
            SettingsSection::of_tab(SettingsTab::Ssh),
            [
                S::SshConnection,
                S::SshSession,
                S::SftpX11,
                S::SshHostKeys,
                S::Gateways
            ],
            "the gateways close the SSH group, as the C#"
        );
        assert_eq!(
            SettingsSection::of_tab(SettingsTab::Rdp),
            [S::RdpDisplay, S::RdpBehavior, S::RdpCertificates]
        );
        assert_eq!(
            SettingsSection::of_tab(SettingsTab::Security),
            [S::Overview, S::Vault, S::Provider, S::ConnectionChecks]
        );
        // One section, as the C# Terminal.
        assert_eq!(
            SettingsSection::of_tab(SettingsTab::Terminal),
            [S::Terminal]
        );
        assert_eq!(
            SettingsSection::first_of(SettingsTab::Ssh),
            S::SshConnection
        );
    }

    #[test]
    fn up_and_down_walk_the_sections_across_the_groups_and_cycle() {
        use SettingsSection as S;
        // Down from the last of a group goes to the first of the next, the name between
        // them being a heading and no stop.
        assert_eq!(S::Onboarding.moved(1), S::Terminal);
        assert_eq!(S::Terminal.moved(1), S::SshConnection);
        assert_eq!(S::SshHostKeys.moved(1), S::Gateways);
        assert_eq!(S::Gateways.moved(1), S::RdpDisplay);
        assert_eq!(S::SshConnection.moved(-1), S::Terminal);
        // Round the ends.
        assert_eq!(S::ConnectionChecks.moved(1), S::Appearance);
        assert_eq!(S::Appearance.moved(-1), S::ConnectionChecks);
        // A full turn comes back, and every section is reached.
        let mut section = S::Appearance;
        let mut seen = Vec::new();
        for _ in 0..SettingsSection::ALL.len() {
            seen.push(section);
            section = section.moved(1);
        }
        assert_eq!(section, S::Appearance);
        assert_eq!(seen, SettingsSection::ALL);
    }

    #[test]
    fn only_the_rows_read_when_a_session_opens_carry_the_glyph() {
        let carrying: Vec<(SettingRow, Applies)> = SettingRow::ALL
            .into_iter()
            .filter_map(|row| row.applies().map(|applies| (row, applies)))
            .collect();
        assert_eq!(
            carrying,
            [
                (SettingRow::PowerShellPolicy, Applies::NewSessions),
                (SettingRow::SessionLogging, Applies::NewSessions),
                (SettingRow::SshAgentPreference, Applies::NewConnections),
                (SettingRow::KeepAlive, Applies::NewConnections),
                (SettingRow::PuttyPath, Applies::NewConnections),
                (SettingRow::SftpAutoOpen, Applies::NewSessions),
                (SettingRow::DockLocalBrowser, Applies::NewSessions),
                (SettingRow::RdpConnectTimeout, Applies::NewConnections),
                (SettingRow::RdpKeepAlive, Applies::NewConnections),
                (SettingRow::RdpBuiltinCompression, Applies::NewConnections),
            ]
        );
        // What is read while it runs applies at once: no glyph.
        for row in [
            SettingRow::SshAutoReconnect,
            SettingRow::SshAutoReconnectAttempts,
            SettingRow::TmoutReset,
            SettingRow::AntiIdle,
            SettingRow::FontSize,
            SettingRow::FontFamily,
            SettingRow::ColorScheme,
            SettingRow::TerminalMinimumContrast,
            SettingRow::CtrlVPaste,
            SettingRow::CtrlKTerminal,
            SettingRow::RdpAutoReconnectAttempts,
        ] {
            assert_eq!(row.applies(), None, "{row:?}");
        }
    }

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
        assert_eq!(
            SettingsCard::Putty.rows(),
            [SettingRow::PuttyPath, SettingRow::SshDefaultMode]
        );
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
    fn the_default_rdp_mode_heads_the_rdp_defaults_card_marked_and_reset_to_embedded() {
        assert_eq!(
            SettingsCard::RdpDefaults.rows(),
            [SettingRow::RdpDefaultMode, SettingRow::RdpDefaults]
        );
        let row = SettingRow::RdpDefaultMode;
        assert_eq!(row.tab(), SettingsTab::Rdp);
        assert!(row.is_marked());
        assert!(
            row.flag(&Settings::default()).is_none(),
            "a list, not a box"
        );
        let changed = Settings {
            rdp_default_mode: RdpMode::External,
            ..Settings::default()
        };
        assert!(row.is_modified(&changed));
        assert!(
            !SettingRow::RdpDefaults.is_modified(&changed),
            "a row of its own"
        );
        assert_eq!(
            row.reset(&changed),
            Some(SettingsMessage::RdpDefaultMode(RdpMode::Embedded))
        );
    }

    #[test]
    fn the_default_ssh_mode_is_on_the_ssh_tab_marked_and_reset_to_embedded() {
        let row = SettingRow::SshDefaultMode;
        assert_eq!(row.tab(), SettingsTab::Ssh);
        assert!(row.is_marked());
        assert!(
            row.flag(&Settings::default()).is_none(),
            "a list, not a box"
        );
        let changed = Settings {
            ssh_default_mode: SshMode::External,
            ..Settings::default()
        };
        assert!(row.is_modified(&changed));
        assert_eq!(
            row.reset(&changed),
            Some(SettingsMessage::SshDefaultMode(SshMode::Embedded))
        );
    }

    #[test]
    fn the_overview_flags_the_csharp_risky_choices_only() {
        let safe = posture(&Settings::default(), false);
        assert!(safe.iter().all(|line| !line.risky));
        let mut settings = Settings::default();
        settings.rdp_defaults.nla = false;
        settings.session_logging = true;
        settings.powershell_execution_policy = ExecutionPolicy::Unrestricted;
        settings.updates.enabled = false;
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
                PostureKey::UpdateChecks,
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
                SettingRow::VaultHelloMaxDays,
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
            [
                SettingRow::VaultHelloMaxDays,
                SettingRow::AutoLock,
                SettingRow::DisconnectOnLock
            ]
        );
        let changed = Settings {
            auto_lock_idle_minutes: 10,
            disconnect_on_lock: true,
            ..Settings::default()
        };
        assert!(SettingRow::AutoLock.is_modified(&changed));
        assert!(SettingRow::DisconnectOnLock.is_modified(&changed));
        // Windows Hello's days, as the C# `VaultHelloMaxDaysBeforeMasterPassword`: never by
        // default, marked once changed.
        let days = Settings {
            windows_hello: heimdall_core::settings::WindowsHello {
                vault_max_days: 30,
                ..heimdall_core::settings::WindowsHello::default()
            },
            ..Settings::default()
        };
        assert!(!SettingRow::VaultHelloMaxDays.is_modified(&Settings::default()));
        assert!(SettingRow::VaultHelloMaxDays.is_modified(&days));
        assert_eq!(
            SettingRow::VaultHelloMaxDays.reset(&days),
            Some(SettingsMessage::VaultHelloMaxDays(0))
        );
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

    #[test]
    fn the_updates_card_is_on_the_general_tab_its_choices_marked_and_reset() {
        assert_eq!(
            SettingsCard::Updates.rows(),
            [
                SettingRow::UpdateChecks,
                SettingRow::UpdateInterval,
                SettingRow::UpdateVersion,
                SettingRow::LegacyMigration
            ]
        );
        assert_eq!(SettingsCard::Updates.tab(), SettingsTab::General);
        assert!(
            !SettingRow::UpdateVersion.is_marked(),
            "the version is no choice"
        );
        assert!(
            !SettingRow::LegacyMigration.is_marked(),
            "the legacy migration offer is no choice"
        );
        let changed = Settings {
            updates: heimdall_core::settings::Updates {
                enabled: false,
                interval_hours: 72,
            },
            ..Settings::default()
        };
        assert!(SettingRow::UpdateChecks.is_modified(&changed));
        assert!(SettingRow::UpdateInterval.is_modified(&changed));
        assert_eq!(
            SettingRow::UpdateChecks.reset(&changed),
            Some(SettingsMessage::UpdateChecks(true))
        );
        assert_eq!(
            SettingRow::UpdateInterval.reset(&changed),
            Some(SettingsMessage::UpdateInterval(24))
        );
        let line = posture(&changed, false)[9];
        assert_eq!(
            (line.key, line.state, line.risky, line.target),
            (
                PostureKey::UpdateChecks,
                PostureState::Off,
                true,
                SettingRow::UpdateChecks
            )
        );
    }

    #[test]
    fn windows_hello_is_on_the_security_tab_its_grace_hangs_from_it_and_its_line_is_reported() {
        assert_eq!(
            SettingsCard::ConnectionChecks.rows(),
            [
                SettingRow::RequireCredentialGuard,
                SettingRow::RequireWindowsHello,
                SettingRow::WindowsHelloGrace
            ]
        );
        assert_eq!(SettingsCard::ConnectionChecks.tab(), SettingsTab::Security);
        let defaults = Settings::default();
        assert!(!SettingRow::WindowsHelloGrace.number_enabled(&defaults));
        assert_eq!(
            SettingRow::RequireWindowsHello.toggle_enabled(&defaults),
            windows_hello::SUPPORTED,
            "offered where Windows Hello exists"
        );
        let changed = Settings {
            windows_hello: heimdall_core::settings::WindowsHello {
                require_on_connect: true,
                grace_minutes: 0,
                ..heimdall_core::settings::WindowsHello::default()
            },
            ..Settings::default()
        };
        assert!(SettingRow::WindowsHelloGrace.number_enabled(&changed));
        assert!(
            SettingRow::RequireWindowsHello.toggle_enabled(&changed),
            "on, it can always be cleared"
        );
        for row in [
            SettingRow::RequireWindowsHello,
            SettingRow::WindowsHelloGrace,
        ] {
            assert!(row.is_marked(), "{row:?}");
            assert!(row.is_modified(&changed), "{row:?}");
        }
        assert_eq!(
            SettingRow::RequireWindowsHello.reset(&changed),
            Some(SettingsMessage::RequireWindowsHello(false))
        );
        assert_eq!(
            SettingRow::WindowsHelloGrace.reset(&changed),
            Some(SettingsMessage::WindowsHelloGraceMinutes(5))
        );
        let line = |settings: &Settings| {
            posture(settings, false)
                .into_iter()
                .find(|line| line.key == PostureKey::WindowsHelloOnConnect)
                .expect("a line")
        };
        assert_eq!(
            (line(&defaults).state, line(&defaults).risky),
            (PostureState::NotRequired, false)
        );
        let required = line(&changed);
        assert_eq!(
            (required.state, required.risky, required.target),
            (
                PostureState::Required,
                false,
                SettingRow::RequireWindowsHello
            )
        );
    }

    #[test]
    fn credential_guard_heads_the_connection_checks_and_its_line_sits_before_windows_hello() {
        let row = SettingRow::RequireCredentialGuard;
        assert_eq!(row.card(), SettingsCard::ConnectionChecks);
        assert_eq!(row.tab(), SettingsTab::Security);
        assert!(row.is_marked());
        let defaults = Settings::default();
        assert_eq!(row.flag(&defaults), Some(false), "off, as the C# default");
        assert!(row.toggle_enabled(&defaults));
        assert!(!row.is_modified(&defaults));
        let changed = Settings {
            require_credential_guard: true,
            ..Settings::default()
        };
        assert!(row.is_modified(&changed));
        assert!(
            !SettingRow::RequireWindowsHello.is_modified(&changed),
            "a row of its own"
        );
        assert_eq!(
            row.reset(&changed),
            Some(SettingsMessage::RequireCredentialGuard(false))
        );
        assert_eq!(
            row.toggled(&defaults, true),
            Some(SettingsMessage::RequireCredentialGuard(true))
        );

        let keys: Vec<PostureKey> = posture(&changed, true)
            .iter()
            .map(|line| line.key)
            .collect();
        assert_eq!(
            keys,
            [
                PostureKey::RdpNla,
                PostureKey::RdpStrictServerAuthentication,
                PostureKey::SessionTranscripts,
                PostureKey::PowerShellExecutionPolicy,
                PostureKey::Vault,
                PostureKey::AutoLock,
                PostureKey::DisconnectOnLock,
                PostureKey::CredentialGuard,
                PostureKey::WindowsHelloOnConnect,
                PostureKey::UpdateChecks,
                PostureKey::KnownHostsSync,
            ],
            "the C# card's order"
        );
        let required = line(&changed, true, PostureKey::CredentialGuard);
        assert_eq!(
            (required.state, required.risky, required.target),
            (
                PostureState::Required,
                false,
                SettingRow::RequireCredentialGuard
            )
        );
        let not_required = line(&defaults, false, PostureKey::CredentialGuard);
        assert_eq!(
            (not_required.state, not_required.risky),
            (PostureState::NotRequired, false),
            "never risky"
        );
    }

    #[test]
    fn the_known_hosts_import_sits_under_the_host_keys_and_is_risky_on_the_last_line() {
        let row = SettingRow::SyncKnownHostsAtStartup;
        assert_eq!(row.card(), SettingsCard::SshTrusted);
        assert_eq!(row.tab(), SettingsTab::Ssh);
        assert_eq!(
            SettingsCard::SshTrusted.rows(),
            [
                SettingRow::HostKeys,
                SettingRow::SyncKnownHostsAtStartup,
                SettingRow::FtpsCertificates,
                SettingRow::VncCertificates,
            ]
        );
        assert!(row.is_marked());
        let defaults = Settings::default();
        assert_eq!(row.flag(&defaults), Some(false), "off, as the C# default");
        assert!(row.toggle_enabled(&defaults));
        assert!(!row.is_modified(&defaults));
        let changed = Settings {
            sync_known_hosts_at_startup: true,
            ..Settings::default()
        };
        assert!(row.is_modified(&changed));
        assert_eq!(
            row.reset(&changed),
            Some(SettingsMessage::SyncKnownHostsAtStartup(false))
        );
        assert_eq!(
            row.toggled(&defaults, true),
            Some(SettingsMessage::SyncKnownHostsAtStartup(true))
        );

        let last = posture(&changed, true)[10];
        assert_eq!(
            (last.key, last.state, last.risky, last.target),
            (
                PostureKey::KnownHostsSync,
                PostureState::On,
                true,
                SettingRow::SyncKnownHostsAtStartup
            ),
            "the last line, as the C# card's"
        );
        let off = line(&defaults, false, PostureKey::KnownHostsSync);
        assert_eq!((off.state, off.risky), (PostureState::Off, false));
    }
}
