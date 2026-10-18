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

//! Every setting a portable settings file carries, and which of its changes an import
//! shows first: as the C# `SettingsImportPreview.SensitiveKeys`, those deciding what runs on
//! this computer, and beyond the C#, those that weaken a security check. A settings file is
//! shared between people, and these are the lines an author with bad intent would hide among
//! harmless ones.

use super::ExecutionPolicy;

/// Why an imported change is shown first, before the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// It decides what Heimdall runs on this computer: a command, an executable, or the
    /// external credential provider that runs one, as every C# `SensitiveKeys` entry.
    RunsOnThisComputer,
    /// It weakens a security check: a server's identity checked less, a device of this
    /// computer shared, a lock or a verification asked less often. Only a change that
    /// weakens is shown first; one that tightens is among the others.
    WeakensSecurity,
}

/// How a change to a setting is reviewed on import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Review {
    /// Shown among the others.
    Plain,
    /// Shown first, whichever way it changes: [`Sensitivity::RunsOnThisComputer`].
    RunsHere,
    /// Shown first when it makes the setting less strict: [`Sensitivity::WeakensSecurity`].
    Weakens(Strictness),
}

/// Which values of a setting that guards a security check are the stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Strictness {
    /// On or off, on the stricter.
    OnIsStricter,
    /// On or off, off the stricter.
    OffIsStricter,
    /// A number, the larger the looser.
    LargerIsLooser,
    /// A number, the larger the looser, and 0 turns the check off, the loosest of all.
    ZeroIsOffLargerIsLooser,
    /// A `PowerShell` execution policy, by name.
    ExecutionPolicy,
}

impl Strictness {
    /// How strict `value` is, the larger the stricter; `None` for a value of another kind.
    fn rank(self, value: &toml::Value) -> Option<i64> {
        match (self, value) {
            (Self::OnIsStricter, toml::Value::Boolean(on)) => Some(i64::from(*on)),
            (Self::OffIsStricter, toml::Value::Boolean(on)) => Some(i64::from(!*on)),
            (Self::ZeroIsOffLargerIsLooser, toml::Value::Integer(0)) => Some(i64::MIN),
            (
                Self::LargerIsLooser | Self::ZeroIsOffLargerIsLooser,
                toml::Value::Integer(number),
            ) => Some(-*number),
            (Self::ExecutionPolicy, toml::Value::String(name)) => {
                Some(policy_rank(ExecutionPolicy::named(name)))
            }
            _ => None,
        }
    }

    /// Whether going from `before` to `after` makes the setting less strict. A value that
    /// cannot be ranked counts as weakening: shown first rather than hidden.
    fn weakens(self, before: Option<&toml::Value>, after: Option<&toml::Value>) -> bool {
        match (
            before.and_then(|value| self.rank(value)),
            after.and_then(|value| self.rank(value)),
        ) {
            (Some(before), Some(after)) => after < before,
            _ => true,
        }
    }
}

/// How strict an execution policy is: every script signed, then the downloaded ones,
/// then `PowerShell`'s own, then every script with a warning, then none checked.
fn policy_rank(policy: ExecutionPolicy) -> i64 {
    match policy {
        ExecutionPolicy::AllSigned => 4,
        ExecutionPolicy::RemoteSigned => 3,
        ExecutionPolicy::Default => 2,
        ExecutionPolicy::Unrestricted => 1,
        ExecutionPolicy::Bypass => 0,
    }
}

/// Shorthands for the table below.
const PLAIN: Review = Review::Plain;
const RUNS: Review = Review::RunsHere;
const ON_STRICT: Review = Review::Weakens(Strictness::OnIsStricter);
const OFF_STRICT: Review = Review::Weakens(Strictness::OffIsStricter);

/// Every setting of the sections a portable settings file carries, as `section.key`, with
/// how a change to it is reviewed. A key a file carries that is not here is not imported,
/// and the preview says so; a test fails for a setting of the file missing from here.
pub(super) const CLASSIFIED: &[(&str, Review)] = &[
    ("terminal.color_scheme", PLAIN),
    ("terminal.broadcast_scope", PLAIN),
    ("terminal.font_size", PLAIN),
    ("terminal.font_family", PLAIN),
    // Scripts run with fewer signature checks.
    (
        "terminal.powershell_execution_policy",
        Review::Weakens(Strictness::ExecutionPolicy),
    ),
    ("terminal.ctrl_v_paste", PLAIN),
    ("terminal.ctrl_k", PLAIN),
    // Every session's transcript written to disk, what was typed and shown included.
    ("session_log.enabled", OFF_STRICT),
    ("session_log.directory", PLAIN),
    ("session_log.retention_days", PLAIN),
    ("general.language", PLAIN),
    ("general.theme", PLAIN),
    ("general.accent", PLAIN),
    ("general.collapse_tunnels_panel", PLAIN),
    ("general.prevent_sleep", PLAIN),
    ("general.max_sessions", PLAIN),
    ("general.diagnostics_log", PLAIN),
    ("general.show_gateway_badge", PLAIN),
    ("general.show_properties_panel", PLAIN),
    ("general.group_tabs_by_folder", PLAIN),
    // The workspace locks later, or never.
    (
        "vault.auto_lock_idle_minutes",
        Review::Weakens(Strictness::ZeroIsOffLargerIsLooser),
    ),
    // The sessions go on running behind the lock.
    ("vault.disconnect_on_lock", ON_STRICT),
    // C# `UseExternalCredentialProvider`.
    ("credential_provider.enabled", RUNS),
    // C# `CredentialProviderType`.
    ("credential_provider.kind", RUNS),
    // C# `CredentialProviderCommand`.
    ("credential_provider.command", RUNS),
    // C# `CredentialProviderUsernameCommand`.
    ("credential_provider.username_command", RUNS),
    // C# `CredentialProviderDatabase`.
    ("credential_provider.database", RUNS),
    // C# `CredentialProviderKeyFile`.
    ("credential_provider.key_file", RUNS),
    ("credential_provider.first_line_only", PLAIN),
    ("credential_provider.timeout_ms", PLAIN),
    ("ssh.auto_reconnect", PLAIN),
    ("ssh.auto_reconnect_attempts", PLAIN),
    ("ssh.auto_reconnect_first_delay", PLAIN),
    ("ssh.auto_reconnect_second_delay", PLAIN),
    ("ssh.auto_reconnect_subsequent_delay", PLAIN),
    ("ssh.anti_idle_interval", PLAIN),
    ("ssh.keep_alive_interval", PLAIN),
    ("ssh.tmout_reset_interval", PLAIN),
    ("ssh.agent_preference", PLAIN),
    // C# `PuttyPath`.
    ("ssh.putty_path", RUNS),
    ("ssh.default_mode", PLAIN),
    // C# `X11ServerPath`.
    ("ssh.x11_server_path", RUNS),
    // An X server started, which remote programs can draw on and read input from.
    ("ssh.x11_auto_start", OFF_STRICT),
    // Host keys trusted from the user's OpenSSH known_hosts without being asked.
    ("ssh.sync_known_hosts_at_startup", OFF_STRICT),
    // This computer's clipboard and devices shared with the remote desktop.
    ("rdp.redirect_clipboard", OFF_STRICT),
    ("rdp.redirect_drives", OFF_STRICT),
    ("rdp.redirect_printers", OFF_STRICT),
    ("rdp.redirect_com_ports", OFF_STRICT),
    ("rdp.redirect_smart_cards", OFF_STRICT),
    ("rdp.redirect_webcam", OFF_STRICT),
    ("rdp.redirect_usb", OFF_STRICT),
    // The user authenticated before the remote desktop is shown, or not.
    ("rdp.nla", ON_STRICT),
    // A server whose identity cannot be checked refused, or not.
    ("rdp.strict_server_authentication", ON_STRICT),
    ("rdp.color_depth", PLAIN),
    ("rdp.audio", PLAIN),
    // This computer's microphone shared with the remote desktop.
    ("rdp.microphone", OFF_STRICT),
    ("rdp.dynamic_resolution", PLAIN),
    ("rdp.multi_monitor", PLAIN),
    ("rdp.bitmap_caching", PLAIN),
    ("rdp.compression", PLAIN),
    ("rdp.hardware_acceleration", PLAIN),
    ("rdp.auto_reconnect", PLAIN),
    ("rdp_session.auto_reconnect_attempts", PLAIN),
    ("rdp_session.connect_timeout", PLAIN),
    ("rdp_session.resize_enable_delay_ms", PLAIN),
    ("rdp_session.keep_alive_interval_ms", PLAIN),
    ("rdp_session.default_width", PLAIN),
    ("rdp_session.default_height", PLAIN),
    ("rdp_session.resolution_presets", PLAIN),
    ("rdp_session.default_mode", PLAIN),
    // Credentials sent to an embedded desktop without Credential Guard protecting them.
    ("rdp_session.require_credential_guard", ON_STRICT),
    // What a server compresses read by the built-in client's bulk decompressors.
    ("rdp_session.builtin_compression", OFF_STRICT),
    ("rdp_session.confirm_disconnect", PLAIN),
    ("rdp_session.indicators_always_expanded", PLAIN),
    // C# `ExternalEditorPath`.
    ("files.external_editor", RUNS),
    ("files.browser_enabled", PLAIN),
    ("files.auto_open_on_ssh", PLAIN),
    ("files.follow_ssh_directory", PLAIN),
    ("files.dock_local_browser", PLAIN),
    ("files.follow_local_directory", PLAIN),
    // C# `ExternalTools`, a `SensitiveKeys` entry: programs started from the menus.
    ("external_tools.tools", RUNS),
    // C# `SharedSessionSources`: files other people write, whose sessions this computer then
    // offers to connect to.
    ("shared_sessions.sources", RUNS),
    ("reachability.enabled", PLAIN),
    ("reachability.interval", PLAIN),
    ("reachability.timeout", PLAIN),
    ("reachability.probes", PLAIN),
    // Security fixes no longer looked for.
    ("updates.enabled", ON_STRICT),
    ("updates.interval_hours", PLAIN),
    // A connection made without Windows Hello verifying the user.
    ("windows_hello.require_on_connect", ON_STRICT),
    // A verification that counts longer.
    (
        "windows_hello.grace_minutes",
        Review::Weakens(Strictness::LargerIsLooser),
    ),
    // The master password asked later, or never again.
    (
        "windows_hello.vault_max_days",
        Review::Weakens(Strictness::ZeroIsOffLargerIsLooser),
    ),
];

/// The review of the setting `section.key`; `None` for one a settings file does not carry.
fn review(section: &str, key: &str) -> Option<Review> {
    CLASSIFIED.iter().find_map(|(name, review)| {
        name.split_once('.')
            .is_some_and(|(at, named)| at == section && named == key)
            .then_some(*review)
    })
}

/// Whether a settings file carries the setting `section.key`: one it does not is not
/// imported.
pub(super) fn carried(section: &str, key: &str) -> bool {
    review(section, key).is_some()
}

/// Why the change of the setting `section.key` from `before` to `after` is shown first;
/// `None` for one shown among the others.
pub(super) fn sensitivity(
    section: &str,
    key: &str,
    before: Option<&toml::Value>,
    after: Option<&toml::Value>,
) -> Option<Sensitivity> {
    match review(section, key)? {
        Review::Plain => None,
        Review::RunsHere => Some(Sensitivity::RunsOnThisComputer),
        Review::Weakens(strictness) => strictness
            .weakens(before, after)
            .then_some(Sensitivity::WeakensSecurity),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::{Language, Settings, TRANSFERRED};
    use super::{CLASSIFIED, Sensitivity, sensitivity};

    /// Every setting of the transferred sections, each optional one given a value so that
    /// it is written.
    fn written() -> BTreeSet<String> {
        let settings = Settings {
            language: Some(Language::French),
            ..Settings::default()
        };
        let mut keys = BTreeSet::new();
        for (section, values) in settings.transferable() {
            let toml::Value::Table(values) = values else {
                panic!("{section}: not a table");
            };
            keys.extend(values.keys().map(|key| format!("{section}.{key}")));
        }
        keys
    }

    #[test]
    fn every_setting_a_settings_file_carries_is_classified_sensitive_or_not() {
        let classified: BTreeSet<String> = CLASSIFIED
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        let written = written();
        let missing: Vec<&String> = written.difference(&classified).collect();
        assert!(
            missing.is_empty(),
            "{missing:?}: shown first on import or not? Say so in CLASSIFIED."
        );
        let stale: Vec<&String> = classified.difference(&written).collect();
        assert!(stale.is_empty(), "{stale:?}: in CLASSIFIED, never written");
        assert_eq!(classified.len(), CLASSIFIED.len(), "a setting listed twice");
    }

    #[test]
    fn every_classified_setting_is_in_a_section_that_travels() {
        for (name, _) in CLASSIFIED {
            let (section, _) = name.split_once('.').expect("section.key");
            assert!(TRANSFERRED.contains(&section), "{name}");
        }
    }

    /// Why the change of `section.key` from `before` to `after` is shown first.
    fn of(name: &str, before: &toml::Value, after: &toml::Value) -> Option<Sensitivity> {
        let (section, key) = name.split_once('.').expect("section.key");
        sensitivity(section, key, Some(before), Some(after))
    }

    const WEAKENS: Option<Sensitivity> = Some(Sensitivity::WeakensSecurity);

    #[test]
    fn a_switch_is_flagged_when_it_weakens_and_not_when_it_tightens() {
        let (on, off) = (toml::Value::Boolean(true), toml::Value::Boolean(false));
        assert_eq!(of("rdp.nla", &on, &off), WEAKENS);
        assert_eq!(of("rdp.nla", &off, &on), None);
        assert_eq!(of("ssh.sync_known_hosts_at_startup", &off, &on), WEAKENS);
        assert_eq!(of("ssh.sync_known_hosts_at_startup", &on, &off), None);
    }

    #[test]
    fn a_number_is_flagged_when_it_loosens_and_zero_turns_the_check_off() {
        let minutes = toml::Value::Integer;
        assert_eq!(
            of("windows_hello.grace_minutes", &minutes(5), &minutes(60)),
            WEAKENS
        );
        assert_eq!(
            of("windows_hello.grace_minutes", &minutes(60), &minutes(0)),
            None,
            "0 asks every time"
        );
        assert_eq!(
            of("vault.auto_lock_idle_minutes", &minutes(15), &minutes(0)),
            WEAKENS,
            "0 never locks"
        );
        assert_eq!(
            of("vault.auto_lock_idle_minutes", &minutes(0), &minutes(60)),
            None
        );
        assert_eq!(
            of("vault.auto_lock_idle_minutes", &minutes(60), &minutes(15)),
            None
        );
    }

    #[test]
    fn an_execution_policy_is_flagged_when_fewer_scripts_are_checked() {
        let policy = |name: &str| toml::Value::String(name.to_owned());
        let name = "terminal.powershell_execution_policy";
        assert_eq!(of(name, &policy("Default"), &policy("Bypass")), WEAKENS);
        assert_eq!(
            of(name, &policy("AllSigned"), &policy("RemoteSigned")),
            WEAKENS
        );
        assert_eq!(of(name, &policy("Unrestricted"), &policy("Default")), None);
        assert_eq!(of(name, &policy("Default"), &policy("AllSigned")), None);
    }

    #[test]
    fn what_runs_here_is_flagged_whichever_way_and_a_plain_setting_never() {
        let text = |value: &str| toml::Value::String(value.to_owned());
        assert_eq!(
            of("ssh.putty_path", &text("/opt/putty"), &text("")),
            Some(Sensitivity::RunsOnThisComputer)
        );
        assert_eq!(
            of("terminal.color_scheme", &text("Nord"), &text("Dracula")),
            None
        );
    }
}
