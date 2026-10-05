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

//! The numbers of the Settings page, as the C# SSH/SFTP Session tab and its session health
//! monitor have them: each typed, then applied with Enter; one out of its range stays typed,
//! its rule said under it.

use heimdall_app::SettingsMessage;
use heimdall_core::settings::{
    self, ANTI_IDLE_INTERVAL_MAX, ANTI_IDLE_INTERVAL_MIN, REACHABILITY_INTERVAL_MAX,
    REACHABILITY_INTERVAL_MIN, REACHABILITY_PROBES_MAX, REACHABILITY_PROBES_MIN,
    REACHABILITY_TIMEOUT_MAX, REACHABILITY_TIMEOUT_MIN, SSH_KEEP_ALIVE_INTERVAL_MAX,
    SSH_KEEP_ALIVE_INTERVAL_MIN, SSH_TMOUT_RESET_INTERVAL_MAX, Settings,
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

    /// How many there are.
    pub(crate) const COUNT: usize = 6;

    /// Its place among them all.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::KeepAlive => 0,
            Self::TmoutReset => 1,
            Self::AntiIdle => 2,
            Self::ReachabilityInterval => 3,
            Self::ReachabilityTimeout => 4,
            Self::ReachabilityProbes => 5,
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
        }
    }

    /// Its unit, after the number; none for a count.
    pub(crate) fn unit(self) -> Option<String> {
        match self {
            Self::KeepAlive | Self::TmoutReset | Self::AntiIdle | Self::ReachabilityInterval => {
                Some(fl!("ui-settings-anti-idle-unit"))
            }
            Self::ReachabilityTimeout => Some(fl!("ui-settings-milliseconds-unit")),
            Self::ReachabilityProbes => None,
        }
    }

    /// What it does, said under it, when the C# says it.
    pub(crate) fn hint(self) -> Option<String> {
        match self {
            Self::KeepAlive => Some(fl!("ui-settings-ssh-keep-alive-hint")),
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
        }
    }
}
