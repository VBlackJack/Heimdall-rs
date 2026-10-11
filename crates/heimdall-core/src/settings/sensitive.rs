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

//! Every setting a portable settings file carries, and which of them an import shows
//! first, as the C# `SettingsImportPreview.SensitiveKeys`: a settings file is shared between
//! people, and the settings deciding what runs on this computer are the lines an author with
//! bad intent would hide among harmless ones.

/// Why an imported setting is shown first, before the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// It decides what Heimdall runs on this computer: a command, an executable, or the
    /// external credential provider that runs one, as every C# `SensitiveKeys` entry.
    RunsOnThisComputer,
}

/// Shorthand for the table below.
const RUNS: Option<Sensitivity> = Some(Sensitivity::RunsOnThisComputer);

/// Every setting of the sections a portable settings file carries, as `section.key`, with
/// why it is shown first, or `None`. A key a file carries that is not here is not imported,
/// and the preview says so; a test fails for a setting of the file missing from here.
pub(super) const CLASSIFIED: &[(&str, Option<Sensitivity>)] = &[
    ("terminal.color_scheme", None),
    ("terminal.broadcast_scope", None),
    ("terminal.font_size", None),
    ("terminal.font_family", None),
    ("terminal.powershell_execution_policy", None),
    ("terminal.ctrl_v_paste", None),
    ("terminal.ctrl_k", None),
    ("session_log.enabled", None),
    ("session_log.directory", None),
    ("session_log.retention_days", None),
    ("general.language", None),
    ("general.theme", None),
    ("general.accent", None),
    ("general.collapse_tunnels_panel", None),
    ("general.prevent_sleep", None),
    ("general.max_sessions", None),
    ("general.diagnostics_log", None),
    ("general.show_gateway_badge", None),
    ("vault.auto_lock_idle_minutes", None),
    ("vault.disconnect_on_lock", None),
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
    ("credential_provider.first_line_only", None),
    ("credential_provider.timeout_ms", None),
    ("ssh.auto_reconnect", None),
    ("ssh.auto_reconnect_attempts", None),
    ("ssh.anti_idle_interval", None),
    ("ssh.keep_alive_interval", None),
    ("ssh.tmout_reset_interval", None),
    ("ssh.agent_preference", None),
    // C# `PuttyPath`.
    ("ssh.putty_path", RUNS),
    ("ssh.default_mode", None),
    // C# `X11ServerPath`.
    ("ssh.x11_server_path", RUNS),
    ("ssh.x11_auto_start", None),
    ("ssh.sync_known_hosts_at_startup", None),
    ("rdp.redirect_clipboard", None),
    ("rdp.redirect_drives", None),
    ("rdp.redirect_printers", None),
    ("rdp.redirect_com_ports", None),
    ("rdp.redirect_smart_cards", None),
    ("rdp.redirect_webcam", None),
    ("rdp.redirect_usb", None),
    ("rdp.nla", None),
    ("rdp.strict_server_authentication", None),
    ("rdp.color_depth", None),
    ("rdp.audio", None),
    ("rdp.microphone", None),
    ("rdp.dynamic_resolution", None),
    ("rdp.multi_monitor", None),
    ("rdp.bitmap_caching", None),
    ("rdp.compression", None),
    ("rdp.hardware_acceleration", None),
    ("rdp.auto_reconnect", None),
    ("rdp_session.auto_reconnect_attempts", None),
    ("rdp_session.connect_timeout", None),
    ("rdp_session.resize_enable_delay_ms", None),
    ("rdp_session.resolution_presets", None),
    ("rdp_session.default_mode", None),
    ("rdp_session.require_credential_guard", None),
    // C# `ExternalEditorPath`.
    ("files.external_editor", RUNS),
    ("files.browser_enabled", None),
    ("files.auto_open_on_ssh", None),
    ("files.follow_ssh_directory", None),
    ("files.dock_local_browser", None),
    ("files.follow_local_directory", None),
    ("reachability.enabled", None),
    ("reachability.interval", None),
    ("reachability.timeout", None),
    ("reachability.probes", None),
    ("updates.enabled", None),
    ("updates.interval_hours", None),
    ("windows_hello.require_on_connect", None),
    ("windows_hello.grace_minutes", None),
    ("windows_hello.vault_max_days", None),
];

/// The entry of the setting `section.key`; `None` for one a settings file does not carry.
fn entry(section: &str, key: &str) -> Option<&'static (&'static str, Option<Sensitivity>)> {
    CLASSIFIED.iter().find(|(name, _)| {
        name.split_once('.')
            .is_some_and(|(at, named)| at == section && named == key)
    })
}

/// Whether a settings file carries the setting `section.key`: one it does not is not
/// imported.
pub(super) fn carried(section: &str, key: &str) -> bool {
    entry(section, key).is_some()
}

/// Why the setting `section.key` is shown first; `None` for one shown among the others.
pub(super) fn sensitivity(section: &str, key: &str) -> Option<Sensitivity> {
    entry(section, key).and_then(|(_, sensitivity)| *sensitivity)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::{Language, Settings, TRANSFERRED};
    use super::CLASSIFIED;

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
}
