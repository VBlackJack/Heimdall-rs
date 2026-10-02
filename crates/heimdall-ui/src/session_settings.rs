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

//! The numbers of the Settings page's session card, as the C# SSH/SFTP Session tab: each
//! typed, then applied with Enter; one out of its range stays typed, its rule said under it.

use heimdall_app::SettingsMessage;
use heimdall_core::settings::{
    self, ANTI_IDLE_INTERVAL_MAX, ANTI_IDLE_INTERVAL_MIN, SSH_KEEP_ALIVE_INTERVAL_MAX,
    SSH_KEEP_ALIVE_INTERVAL_MIN, SSH_TMOUT_RESET_INTERVAL_MAX, Settings,
};

use crate::i18n::fl;

/// A number of the session card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionField {
    /// Seconds between two SSH keep-alives.
    KeepAlive,
    /// Seconds of no input before an SSH shell's `TMOUT` reset.
    TmoutReset,
    /// Seconds between two anti-idle keys of an RDP session.
    AntiIdle,
}

impl SessionField {
    /// Every field, in the card's order, the C# tab's.
    pub const ALL: [Self; 3] = [Self::KeepAlive, Self::TmoutReset, Self::AntiIdle];

    /// Its place in [`SessionField::ALL`].
    pub(crate) fn index(self) -> usize {
        match self {
            Self::KeepAlive => 0,
            Self::TmoutReset => 1,
            Self::AntiIdle => 2,
        }
    }

    /// Its label.
    pub(crate) fn label(self) -> String {
        match self {
            Self::KeepAlive => fl!("ui-settings-ssh-keep-alive-interval"),
            Self::TmoutReset => fl!("ui-settings-ssh-tmout-reset-interval"),
            Self::AntiIdle => fl!("ui-settings-anti-idle-interval"),
        }
    }

    /// What it does, said under it, when the C# says it.
    pub(crate) fn hint(self) -> Option<String> {
        match self {
            Self::KeepAlive => Some(fl!("ui-settings-ssh-keep-alive-hint")),
            Self::TmoutReset | Self::AntiIdle => None,
        }
    }

    /// Its value in `settings`.
    pub(crate) fn value(self, settings: &Settings) -> u32 {
        match self {
            Self::KeepAlive => settings.ssh_keep_alive_interval,
            Self::TmoutReset => settings.ssh_tmout_reset_interval,
            Self::AntiIdle => settings.anti_idle_interval,
        }
    }

    /// Whether `seconds` is in its range.
    pub(crate) fn accepted(self, seconds: u32) -> bool {
        match self {
            Self::KeepAlive => settings::ssh_keep_alive_interval_accepted(seconds),
            Self::TmoutReset => settings::ssh_tmout_reset_interval_accepted(seconds),
            Self::AntiIdle => settings::anti_idle_interval_accepted(seconds),
        }
    }

    /// The change that sets it to `seconds`.
    pub(crate) fn applied(self, seconds: u32) -> SettingsMessage {
        match self {
            Self::KeepAlive => SettingsMessage::SshKeepAliveInterval(seconds),
            Self::TmoutReset => SettingsMessage::SshTmoutResetInterval(seconds),
            Self::AntiIdle => SettingsMessage::AntiIdleInterval(seconds),
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
        }
    }
}
