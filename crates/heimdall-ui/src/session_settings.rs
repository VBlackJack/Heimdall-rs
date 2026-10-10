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

//! The numbers of the Settings page, as the C# SSH/SFTP Session tab, its session health
//! monitor, its session logging, its idle auto-lock, its update checks, its Windows Hello
//! grace, the days Windows Hello unlocks the vault and its RDP Behavior numbers have them: each typed, then applied
//! with Enter; one out of its range stays typed, its rule said under it.

use heimdall_app::SettingsMessage;
use heimdall_core::settings::{
    self, ANTI_IDLE_INTERVAL_MAX, ANTI_IDLE_INTERVAL_MIN, AUTO_LOCK_IDLE_MINUTES_MAX,
    AUTO_LOCK_IDLE_MINUTES_OFF, REACHABILITY_INTERVAL_MAX, REACHABILITY_INTERVAL_MIN,
    REACHABILITY_PROBES_MAX, REACHABILITY_PROBES_MIN, REACHABILITY_TIMEOUT_MAX,
    REACHABILITY_TIMEOUT_MIN, SESSION_LOG_RETENTION_DAYS_MAX, SESSION_LOG_RETENTION_DAYS_MIN,
    SSH_KEEP_ALIVE_INTERVAL_MAX, SSH_KEEP_ALIVE_INTERVAL_MIN, SSH_TMOUT_RESET_INTERVAL_MAX,
    Settings, UPDATE_INTERVAL_HOURS_MAX, UPDATE_INTERVAL_HOURS_MIN,
    WINDOWS_HELLO_GRACE_MINUTES_MAX, WINDOWS_HELLO_GRACE_MINUTES_NONE,
    WINDOWS_HELLO_VAULT_MAX_DAYS_MAX, WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER,
};

use heimdall_core::settings::{
    RDP_KEEP_ALIVE_INTERVAL_MAX_MS, RDP_KEEP_ALIVE_INTERVAL_MIN_MS, RDP_RESIZE_ENABLE_DELAY_MAX_MS,
    RDP_RESIZE_ENABLE_DELAY_MIN_MS,
};

use crate::i18n::fl;

/// A number of the Settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionField {
    /// Seconds between two SSH keep-alives.
    KeepAlive,
    /// Seconds of no input before an SSH shell's `TMOUT` reset.
    TmoutReset,
    /// Seconds between two anti-idle keys of an RDP session.
    AntiIdle,
    /// Seconds between two background checks of every server.
    ReachabilityInterval,
    /// Milliseconds a server has to answer one.
    ReachabilityTimeout,
    /// Servers it dials at once.
    ReachabilityProbes,
    /// Days a session transcript is kept, 0 for every one.
    TranscriptRetention,
    /// Minutes without input before the workspace locks, 0 for never.
    AutoLock,
    /// Hours between two looks for a newer release.
    UpdateInterval,
    /// Minutes a Windows Hello verification counts, 0 for every time.
    WindowsHelloGrace,
    /// Days Windows Hello unlocks the vault before the master password is asked again, 0
    /// for never.
    VaultHelloMaxDays,
    /// Milliseconds an RDP desktop following its tab waits after connecting, 0 for none.
    RdpResizeDelay,
    /// Milliseconds between two keep-alives of an embedded RDP connection.
    RdpKeepAlive,
}

impl SessionField {
    /// The session card's, in the C# tab's order.
    pub const SESSION: [Self; 3] = [Self::KeepAlive, Self::TmoutReset, Self::AntiIdle];

    /// The session health monitor's, in the C# order.
    pub const REACHABILITY: [Self; 3] = [
        Self::ReachabilityInterval,
        Self::ReachabilityTimeout,
        Self::ReachabilityProbes,
    ];

    /// The session logging's, as the C# shows it under the transcripts' folder.
    pub const TRANSCRIPTS: [Self; 1] = [Self::TranscriptRetention];

    /// How many there are.
    pub(crate) const COUNT: usize = 13;

    /// Its place among them all.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::KeepAlive => 0,
            Self::TmoutReset => 1,
            Self::AntiIdle => 2,
            Self::ReachabilityInterval => 3,
            Self::ReachabilityTimeout => 4,
            Self::ReachabilityProbes => 5,
            Self::TranscriptRetention => 6,
            Self::AutoLock => 7,
            Self::UpdateInterval => 8,
            Self::WindowsHelloGrace => 9,
            Self::VaultHelloMaxDays => 10,
            Self::RdpResizeDelay => 11,
            Self::RdpKeepAlive => 12,
        }
    }

    /// Its label.
    pub(crate) fn label(self) -> String {
        match self {
            Self::KeepAlive => fl!("ui-settings-ssh-keep-alive-interval"),
            Self::TmoutReset => fl!("ui-settings-ssh-tmout-reset-interval"),
            Self::AntiIdle => fl!("ui-settings-anti-idle-interval"),
            Self::ReachabilityInterval => fl!("ui-settings-reachability-interval"),
            Self::ReachabilityTimeout => fl!("ui-settings-reachability-timeout"),
            Self::ReachabilityProbes => fl!("ui-settings-reachability-probes"),
            Self::TranscriptRetention => fl!("ui-settings-session-log-retention"),
            Self::AutoLock => fl!("ui-settings-auto-lock"),
            Self::UpdateInterval => fl!("ui-settings-updates-interval"),
            Self::WindowsHelloGrace => fl!("ui-settings-windows-hello-grace"),
            Self::VaultHelloMaxDays => fl!("ui-settings-vault-hello-max-days"),
            Self::RdpResizeDelay => fl!("ui-settings-rdp-resize-delay"),
            Self::RdpKeepAlive => fl!("ui-settings-rdp-keep-alive-interval"),
        }
    }

    /// Its unit, after the number; none for a count.
    pub(crate) fn unit(self) -> Option<String> {
        match self {
            Self::KeepAlive | Self::TmoutReset | Self::AntiIdle | Self::ReachabilityInterval => {
                Some(fl!("ui-settings-anti-idle-unit"))
            }
            Self::ReachabilityTimeout | Self::RdpResizeDelay | Self::RdpKeepAlive => {
                Some(fl!("ui-settings-milliseconds-unit"))
            }
            Self::TranscriptRetention | Self::VaultHelloMaxDays => {
                Some(fl!("ui-settings-days-unit"))
            }
            Self::AutoLock | Self::WindowsHelloGrace => Some(fl!("ui-settings-minutes-unit")),
            Self::UpdateInterval => Some(fl!("ui-settings-hours-unit")),
            Self::ReachabilityProbes => None,
        }
    }

    /// What it does, said under it, when the C# says it.
    pub(crate) fn hint(self) -> Option<String> {
        match self {
            Self::KeepAlive => Some(fl!("ui-settings-ssh-keep-alive-hint")),
            Self::TranscriptRetention => Some(fl!("ui-settings-session-log-retention-hint")),
            Self::AutoLock => Some(fl!("ui-settings-auto-lock-hint")),
            Self::VaultHelloMaxDays => Some(fl!("ui-settings-vault-hello-max-days-hint")),
            _ => None,
        }
    }

    /// Its value in `settings`.
    pub(crate) fn value(self, settings: &Settings) -> u32 {
        match self {
            Self::KeepAlive => settings.ssh_keep_alive_interval,
            Self::TmoutReset => settings.ssh_tmout_reset_interval,
            Self::AntiIdle => settings.anti_idle_interval,
            Self::ReachabilityInterval => settings.reachability.interval,
            Self::ReachabilityTimeout => settings.reachability.timeout,
            Self::ReachabilityProbes => settings.reachability.probes,
            Self::TranscriptRetention => settings.session_log_retention_days,
            Self::AutoLock => settings.auto_lock_idle_minutes,
            Self::UpdateInterval => settings.updates.interval_hours,
            Self::WindowsHelloGrace => settings.windows_hello.grace_minutes,
            Self::VaultHelloMaxDays => settings.windows_hello.vault_max_days,
            Self::RdpResizeDelay => settings.rdp_resize_enable_delay_ms,
            Self::RdpKeepAlive => settings.rdp_keep_alive_interval_ms,
        }
    }

    /// Whether `value` is in its range.
    pub(crate) fn accepted(self, value: u32) -> bool {
        match self {
            Self::KeepAlive => settings::ssh_keep_alive_interval_accepted(value),
            Self::TmoutReset => settings::ssh_tmout_reset_interval_accepted(value),
            Self::AntiIdle => settings::anti_idle_interval_accepted(value),
            Self::ReachabilityInterval => settings::reachability_interval_accepted(value),
            Self::ReachabilityTimeout => settings::reachability_timeout_accepted(value),
            Self::ReachabilityProbes => settings::reachability_probes_accepted(value),
            Self::TranscriptRetention => settings::session_log_retention_days_accepted(value),
            Self::AutoLock => settings::auto_lock_idle_minutes_accepted(value),
            Self::UpdateInterval => settings::update_interval_accepted(value),
            Self::WindowsHelloGrace => settings::windows_hello_grace_minutes_accepted(value),
            Self::VaultHelloMaxDays => settings::windows_hello_vault_max_days_accepted(value),
            Self::RdpResizeDelay => settings::rdp_resize_enable_delay_accepted(value),
            Self::RdpKeepAlive => settings::rdp_keep_alive_interval_accepted(value),
        }
    }

    /// The change that sets it to `value`.
    pub(crate) fn applied(self, value: u32) -> SettingsMessage {
        match self {
            Self::KeepAlive => SettingsMessage::SshKeepAliveInterval(value),
            Self::TmoutReset => SettingsMessage::SshTmoutResetInterval(value),
            Self::AntiIdle => SettingsMessage::AntiIdleInterval(value),
            Self::ReachabilityInterval => SettingsMessage::ReachabilityInterval(value),
            Self::ReachabilityTimeout => SettingsMessage::ReachabilityTimeout(value),
            Self::ReachabilityProbes => SettingsMessage::ReachabilityProbes(value),
            Self::TranscriptRetention => SettingsMessage::SessionLogRetentionDays(value),
            Self::AutoLock => SettingsMessage::AutoLockIdleMinutes(value),
            Self::UpdateInterval => SettingsMessage::UpdateInterval(value),
            Self::WindowsHelloGrace => SettingsMessage::WindowsHelloGraceMinutes(value),
            Self::VaultHelloMaxDays => SettingsMessage::VaultHelloMaxDays(value),
            Self::RdpResizeDelay => SettingsMessage::RdpResizeEnableDelay(value),
            Self::RdpKeepAlive => SettingsMessage::RdpKeepAliveInterval(value),
        }
    }

    /// Its range, said when a value out of it is typed.
    pub(crate) fn refusal(self) -> String {
        match self {
            Self::KeepAlive => fl!(
                "ui-settings-ssh-keep-alive-refused",
                min = SSH_KEEP_ALIVE_INTERVAL_MIN,
                max = SSH_KEEP_ALIVE_INTERVAL_MAX
            ),
            Self::TmoutReset => fl!(
                "ui-settings-ssh-tmout-reset-refused",
                min = 0,
                max = SSH_TMOUT_RESET_INTERVAL_MAX
            ),
            Self::AntiIdle => fl!(
                "ui-settings-anti-idle-refused",
                min = ANTI_IDLE_INTERVAL_MIN,
                max = ANTI_IDLE_INTERVAL_MAX
            ),
            Self::ReachabilityInterval => fl!(
                "ui-settings-reachability-interval-refused",
                min = REACHABILITY_INTERVAL_MIN,
                max = REACHABILITY_INTERVAL_MAX
            ),
            Self::ReachabilityTimeout => fl!(
                "ui-settings-reachability-timeout-refused",
                min = REACHABILITY_TIMEOUT_MIN,
                max = REACHABILITY_TIMEOUT_MAX
            ),
            Self::ReachabilityProbes => fl!(
                "ui-settings-reachability-probes-refused",
                min = REACHABILITY_PROBES_MIN,
                max = REACHABILITY_PROBES_MAX
            ),
            Self::TranscriptRetention => fl!(
                "ui-settings-session-log-retention-refused",
                min = SESSION_LOG_RETENTION_DAYS_MIN,
                max = SESSION_LOG_RETENTION_DAYS_MAX
            ),
            Self::AutoLock => fl!(
                "ui-settings-auto-lock-refused",
                min = AUTO_LOCK_IDLE_MINUTES_OFF,
                max = AUTO_LOCK_IDLE_MINUTES_MAX
            ),
            Self::UpdateInterval => fl!(
                "ui-settings-updates-interval-refused",
                min = UPDATE_INTERVAL_HOURS_MIN,
                max = UPDATE_INTERVAL_HOURS_MAX
            ),
            Self::WindowsHelloGrace => fl!(
                "ui-settings-windows-hello-grace-refused",
                min = WINDOWS_HELLO_GRACE_MINUTES_NONE,
                max = WINDOWS_HELLO_GRACE_MINUTES_MAX
            ),
            Self::VaultHelloMaxDays => fl!(
                "ui-settings-vault-hello-max-days-refused",
                min = WINDOWS_HELLO_VAULT_MAX_DAYS_NEVER,
                max = WINDOWS_HELLO_VAULT_MAX_DAYS_MAX
            ),
            Self::RdpResizeDelay => fl!(
                "ui-settings-rdp-resize-delay-refused",
                min = RDP_RESIZE_ENABLE_DELAY_MIN_MS,
                max = RDP_RESIZE_ENABLE_DELAY_MAX_MS
            ),
            Self::RdpKeepAlive => fl!(
                "ui-settings-rdp-keep-alive-interval-refused",
                min = RDP_KEEP_ALIVE_INTERVAL_MIN_MS,
                max = RDP_KEEP_ALIVE_INTERVAL_MAX_MS
            ),
        }
    }
}
