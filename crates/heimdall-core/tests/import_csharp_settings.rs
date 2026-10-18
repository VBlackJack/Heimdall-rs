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

//! The preferences of the C# Heimdall's `settings.json` and of its "Export settings" file,
//! read from JSON written as the C# writes it.

use std::time::Duration;

use heimdall_core::credential_provider::ProviderKind;
use heimdall_core::import::csharp_settings::{
    CsharpSettingsError, PreferenceOutcome, Preferences, carried_keys, preferences,
};
use heimdall_core::profile::{AudioPlayback, ColorDepth};
use heimdall_core::settings::{
    Accent, AgentPreference, AppTheme, BroadcastScope, ColorScheme, Language, Settings,
    TransferError, UpdateCheck,
};

/// The C# `settings.json` holding `fields`, in camel case as the C# writes it.
fn settings_json(fields: &str) -> String {
    format!(r#"{{"schemaVersion": 1, {fields}}}"#)
}

/// The C# "Export settings" file holding `settings`, in Pascal case as the C# writes it.
fn export_file(settings: &str) -> String {
    format!(r#"{{"format": "heimdall-settings", "version": 1, "settings": {{{settings}}}}}"#)
}

/// The preferences of `text` over the default settings.
fn read(text: &str) -> Preferences {
    preferences(text, &Settings::default()).expect("C# settings")
}

/// The outcome of the preference `key`, `None` when it is not listed.
fn outcome(read: &Preferences, key: &str) -> Option<PreferenceOutcome> {
    read.rows
        .iter()
        .find(|row| row.key == key)
        .map(|row| row.outcome)
}

#[test]
fn the_preferences_the_csharp_user_changed_are_laid_over_the_settings() {
    let read = read(&settings_json(
        r#""defaultTheme": "Vesper", "accentTint": "pink", "defaultLocale": "fr",
           "terminalFontFamily": "Cascadia Mono", "terminalFontSize": 16,
           "terminalColorScheme": "Solarized Dark", "sshAutoReconnect": true,
           "sshAutoReconnectAttempts": 5, "rdpAutoReconnectMaxAttempts": 7,
           "rdpConnectWatchdogTimeoutMs": 30000, "rdpKeepAliveIntervalMs": 120000,
           "sshKeepAliveIntervalSeconds": 45, "autoLockIdleMinutes": 15,
           "disconnectOnLock": true, "useExternalCredentialProvider": true,
           "credentialProviderType": "WindowsCredentialManager",
           "credentialProviderTimeoutMs": 20000, "sessionLoggingEnabled": true,
           "sessionLogDirectory": "D:\\logs\\heimdall", "sessionLogRetentionDays": 30,
           "updateCheckIntervalHours": 48, "broadcastScope": "SelectedPanes",
           "sshAgentPreference": 3, "rdpDefaultAudioMode": 1, "rdpDefaultColorDepth": 16,
           "rdpDefaultRedirectDrives": true, "rdpResolutionPresets": ["1280x720", "1024x768"],
           "windowsHelloGraceMinutes": 30, "maxEmbeddedSessions": 12"#,
    ));
    let settings = &read.settings;
    assert_eq!(settings.theme, AppTheme::Vesper);
    assert_eq!(settings.accent, Accent::Pink, "a name read without case");
    assert_eq!(settings.language, Some(Language::French));
    assert_eq!(settings.terminal_font_family, "Cascadia Mono");
    assert_eq!(settings.terminal_font_size, 16);
    assert_eq!(settings.color_scheme, ColorScheme::SolarizedDark);
    assert!(settings.ssh_auto_reconnect);
    assert_eq!(settings.ssh_auto_reconnect_attempts, 5);
    assert_eq!(settings.rdp_auto_reconnect_attempts, 7);
    assert_eq!(settings.rdp_connect_timeout, 30, "milliseconds as seconds");
    assert_eq!(settings.rdp_keep_alive_interval_ms, 120_000);
    assert_eq!(settings.ssh_keep_alive_interval, 45);
    assert_eq!(settings.auto_lock_idle_minutes, 15);
    assert!(settings.disconnect_on_lock);
    assert!(settings.credential_provider.enabled);
    assert_eq!(
        settings.credential_provider.kind,
        ProviderKind::WindowsCredentialManager
    );
    assert_eq!(
        settings.credential_provider.timeout,
        Duration::from_secs(20)
    );
    assert!(settings.session_logging);
    assert_eq!(settings.session_log_directory, "D:\\logs\\heimdall");
    assert_eq!(settings.session_log_retention_days, 30);
    assert_eq!(settings.updates.interval_hours, 48);
    assert_eq!(settings.broadcast_scope, BroadcastScope::SelectedTabs);
    assert_eq!(
        settings.ssh_agent_preference,
        AgentPreference::PageantOnly,
        "an enum by its C# number"
    );
    assert_eq!(settings.rdp_defaults.audio, AudioPlayback::Local);
    assert_eq!(settings.rdp_defaults.color_depth, ColorDepth::Bpp16);
    assert!(settings.rdp_defaults.redirect_drives);
    assert_eq!(settings.rdp_resolution_presets, [(1280, 720), (1024, 768)]);
    assert_eq!(settings.windows_hello.grace_minutes, 30);
    assert_eq!(settings.max_sessions, 12);
    assert_eq!(read.count(PreferenceOutcome::Taken), 29);
    assert_eq!(read.rows.len(), 29, "nothing left out: {:?}", read.rows);
    assert!(!read.export_file);
}

#[test]
fn a_value_at_the_csharp_default_says_nothing_of_the_user() {
    // What the C# writes for a user who never changed them: the defaults of `AppSettings`.
    let read = read(&settings_json(
        r#""defaultTheme": "Drakul", "defaultLocale": "en", "terminalFontFamily": "Consolas",
           "terminalFontSize": 14, "terminalColorScheme": "Dracula",
           "externalEditorPath": "%windir%\\system32\\notepad.exe",
           "sessionLogDirectory": "logs\\sessions", "rdpConnectWatchdogTimeoutMs": 45000,
           "puttyPath": null, "rdpResolutionPresets": ["1920x1080", "1680x1050", "1600x900",
           "1440x900", "1366x768", "1280x1024", "1280x720", "1024x768", "2560x1440",
           "3840x2160"], "rdpDefaultColorDepth": 32, "credentialProviderCommand": """#,
    ));
    assert_eq!(read.rows, [], "nothing to say");
    assert_eq!(read.settings, Settings::default());
}

#[test]
fn a_preference_changed_here_is_kept_and_said() {
    let current = Settings {
        theme: AppTheme::Mortis,
        terminal_font_size: 18,
        ..Settings::default()
    };
    let read = preferences(
        &settings_json(r#""defaultTheme": "Vesper", "terminalFontSize": 18, "accentTint": "Red""#),
        &current,
    )
    .expect("C# settings");
    assert_eq!(
        read.settings.theme,
        AppTheme::Mortis,
        "the choice made here stays"
    );
    assert_eq!(
        outcome(&read, "DefaultTheme"),
        Some(PreferenceOutcome::KeptHere)
    );
    assert_eq!(
        outcome(&read, "TerminalFontSize"),
        None,
        "the same value here: nothing to say"
    );
    assert_eq!(
        read.settings.accent,
        Accent::Red,
        "one still at its default here"
    );
    assert_eq!(outcome(&read, "AccentTint"), Some(PreferenceOutcome::Taken));
}

#[test]
fn a_value_heimdall_rs_does_not_read_is_invalid_and_changes_nothing() {
    let read = read(&settings_json(
        r#""defaultTheme": "Neon", "terminalFontSize": 200, "rdpDefaultColorDepth": 8,
           "broadcastScope": 7, "defaultLocale": "de", "sshKeepAliveIntervalSeconds": "45",
           "rdpResolutionPresets": ["1280x720", "wide"], "rdpConnectWatchdogTimeoutMs": 1200,
           "sshAutoReconnect": "yes", "rdpDefaultAudioMode": 5, "terminalFontFamily": 12,
           "credentialProviderTimeoutMs": 500"#,
    ));
    assert_eq!(read.settings, Settings::default());
    assert_eq!(
        read.count(PreferenceOutcome::Invalid),
        12,
        "{:?}",
        read.rows
    );
    assert_eq!(read.rows.len(), 12);
}

#[test]
fn secrets_are_never_carried_over_and_are_named() {
    let read = read(&settings_json(
        r#""pinHash": "c2VjcmV0", "pinSalt": "", "hmacKey": "a2V5",
           "cmdLibGitSyncToken": "ghp_0123456789", "credentialProviderUnlockSecretEncrypted":
           "AQAAANCMnd8BFdERjHoAwE", "vaultWrappedDek": "AQAA", "vaultHelloSalt": null,
           "credentialProviderCommand": "keepassxc-cli show -s \"{Database}\" \"{Title}\"""#,
    ));
    let secrets: Vec<&str> = read.named(PreferenceOutcome::Secret).collect();
    assert_eq!(
        secrets,
        [
            "PinHash",
            "HmacKey",
            "CmdLibGitSyncToken",
            "CredentialProviderUnlockSecretEncrypted",
            "VaultWrappedDek"
        ],
        "an empty or null one holds nothing"
    );
    assert_eq!(read.settings.pin, None);
    // A command template is no secret: carried over, shown first on review.
    assert_eq!(
        read.settings.credential_provider.command,
        r#"keepassxc-cli show -s "{Database}" "{Title}""#
    );
    let review = Settings::default().csharp_import(&read);
    assert!(
        review
            .changes
            .iter()
            .all(|change| !change.key.starts_with("pin")),
        "{:?}",
        review.changes
    );
    assert!(review.ignored.iter().any(|name| name == "PinHash"));
    assert!(
        review
            .sensitive()
            .any(|change| change.key == "credential_provider.command")
    );
}

#[test]
fn a_path_of_the_csharp_installation_is_not_carried_over() {
    let read = read(&settings_json(
        r#""sessionLogDirectory": "logs\\custom", "puttyPath": "tools\\putty.exe",
           "x11ServerPath": "C:\\Program Files\\VcXsrv\\vcxsrv.exe",
           "externalEditorPath": "code""#,
    ));
    // Neither the editor's launch nor PuTTY's looks a bare name up on PATH here.
    assert_eq!(
        outcome(&read, "ExternalEditorPath"),
        Some(PreferenceOutcome::RelativePath)
    );
    assert_eq!(
        outcome(&read, "SessionLogDirectory"),
        Some(PreferenceOutcome::RelativePath)
    );
    assert_eq!(
        outcome(&read, "PuttyPath"),
        Some(PreferenceOutcome::RelativePath)
    );
    assert_eq!(
        read.settings.x11_server_path,
        "C:\\Program Files\\VcXsrv\\vcxsrv.exe"
    );
    assert_eq!(read.settings.external_editor, "");
    assert_eq!(read.settings.putty_path, "");

    let read = preferences(
        &settings_json(
            r#""externalEditorPath": "%ProgramFiles%\\Notepad++\\notepad++.exe",
           "puttyPath": "%ProgramFiles%\\PuTTY\\putty.exe", "x11ServerPath": "vcxsrv.exe""#,
        ),
        &Settings::default(),
    )
    .expect("C# settings");
    assert_eq!(
        read.settings.external_editor, "%ProgramFiles%\\Notepad++\\notepad++.exe",
        "the editor's launch expands it"
    );
    assert_eq!(
        outcome(&read, "PuttyPath"),
        Some(PreferenceOutcome::RelativePath),
        "PuTTY is taken from an absolute path only"
    );
    assert_eq!(
        outcome(&read, "X11ServerPath"),
        Some(PreferenceOutcome::RelativePath)
    );
}

#[test]
fn the_changes_alone_are_laid_over_the_settings_as_they_are_when_taken() {
    let read = Settings::default()
        .import(&export_file(r#""DefaultTheme": "Vesper""#))
        .expect("a C# export file");
    // Written meanwhile, without the user: kept.
    let now = Settings {
        update_check: UpdateCheck {
            last_check: None,
            skipped: Some("v2026.101701".to_owned()),
        },
        ..Settings::default()
    };
    let taken = now.with_changes_of(&read).expect("settings");
    assert_eq!(taken.theme, AppTheme::Vesper);
    assert_eq!(taken.update_check, now.update_check);
}

#[test]
fn an_export_file_starting_with_a_byte_order_mark_is_read() {
    let text = format!("\u{feff}{}", export_file(r#""DefaultTheme": "Vesper""#));
    let read = Settings::default().import(&text).expect("a C# export file");
    assert_eq!(read.settings.theme, AppTheme::Vesper);
}

#[test]
fn a_malformed_file_is_an_error_of_its_own() {
    for text in [r#"{"defaultTheme": "Vesper""#, "", "not json"] {
        assert!(
            matches!(
                preferences(text, &Settings::default()),
                Err(CsharpSettingsError::NotJson(_))
            ),
            "{text}"
        );
    }
    assert!(matches!(
        preferences("[1, 2]", &Settings::default()),
        Err(CsharpSettingsError::NotSettings)
    ));
}

#[test]
fn the_export_file_is_read_in_pascal_case_and_says_what_has_no_equivalent() {
    let read = read(&export_file(
        r#""DefaultTheme": "Vesper", "NotesSidebarWidth": 400, "PinHash": "c2VjcmV0",
           "TerminalFontSize": 16"#,
    ));
    assert!(read.export_file);
    assert_eq!(read.settings.theme, AppTheme::Vesper);
    assert_eq!(read.settings.terminal_font_size, 16);
    assert_eq!(
        outcome(&read, "NotesSidebarWidth"),
        Some(PreferenceOutcome::NoEquivalent)
    );
    assert_eq!(outcome(&read, "PinHash"), Some(PreferenceOutcome::Secret));

    let newer = r#"{"format": "heimdall-settings", "version": 2, "settings": {}}"#;
    assert!(matches!(
        preferences(newer, &Settings::default()),
        Err(CsharpSettingsError::Newer(2))
    ));
    let other = r#"{"format": "something-else", "version": 1, "settings": {}}"#;
    assert!(matches!(
        preferences(other, &Settings::default()),
        Err(CsharpSettingsError::NotSettings)
    ));
}

#[test]
fn the_settings_import_reads_the_csharp_export_file_and_only_it() {
    let current = Settings {
        accent: Accent::Green,
        ..Settings::default()
    };
    let read = current
        .import(&export_file(
            r#""DefaultTheme": "Vesper", "AccentTint": "Red", "PinHash": "c2VjcmV0",
               "SidebarWidth": 300"#,
        ))
        .expect("a C# export file");
    assert_eq!(read.settings.theme, AppTheme::Vesper);
    assert_eq!(read.settings.accent, Accent::Green, "changed here: kept");
    let changed: Vec<&str> = read
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert_eq!(changed, ["general.theme"]);
    assert_eq!(read.ignored, ["AccentTint", "PinHash", "SidebarWidth"]);

    assert_eq!(
        current.import(&settings_json(r#""defaultTheme": "Vesper""#)),
        Err(TransferError::NotSettings),
        "the C# settings.json is read by the import from the C# Heimdall"
    );
    assert_eq!(
        current.import(r#"{"format": "heimdall-settings", "version": 3, "settings": {}}"#),
        Err(TransferError::Newer)
    );
    assert_eq!(current.import("{ broken"), Err(TransferError::NotSettings));
}

/// One valid value for each C# preference carried over, none at its C# default.
const EVERY_PREFERENCE: &[(&str, &str)] = &[
    ("DefaultLocale", r#""es""#),
    ("DefaultTheme", r#""Vesper""#),
    ("AccentTint", r#""Red""#),
    ("UpdateCheckEnabled", "false"),
    ("UpdateCheckIntervalHours", "48"),
    ("CredentialProviderTimeoutMs", "20000"),
    ("RdpResizeEnableDelayMs", "2000"),
    ("RdpConnectWatchdogTimeoutMs", "0"),
    ("RdpAutoReconnectMaxAttempts", "5"),
    ("RdpKeepAliveIntervalMs", "30000"),
    ("SshKeepAliveIntervalSeconds", "60"),
    ("EnableLogging", "false"),
    ("RequireCredentialGuard", "true"),
    ("TerminalFontFamily", r#""Cascadia Mono""#),
    ("TerminalFontSize", "18"),
    ("TerminalColorScheme", r#""Nord""#),
    ("PowerShellExecutionPolicy", r#""RemoteSigned""#),
    ("SshDefaultMode", r#""External""#),
    ("SshAgentPreference", r#""AutoPageantFirst""#),
    ("SyncKnownHostsAtStartup", "true"),
    ("AntiIdleIntervalSeconds", "0"),
    ("SshTmoutResetIntervalSeconds", "120"),
    ("SshAutoReconnect", "true"),
    ("SshAutoReconnectAttempts", "6"),
    ("RdpDefaultMode", r#""External""#),
    ("RdpDefaultRedirectClipboard", "false"),
    ("RdpDefaultRedirectDrives", "true"),
    ("RdpDefaultRedirectPrinters", "true"),
    ("RdpDefaultRedirectComPorts", "true"),
    ("RdpDefaultRedirectSmartCards", "true"),
    ("RdpDefaultRedirectWebcam", "true"),
    ("RdpDefaultRedirectUsb", "true"),
    ("RdpDefaultAudioMode", "2"),
    ("RdpDefaultAudioCapture", "true"),
    ("RdpDefaultMultiMonitor", "true"),
    ("RdpDefaultDynamicResolution", "false"),
    ("RdpDefaultNla", "false"),
    ("RdpDefaultStrictServerAuthentication", "true"),
    ("RdpDefaultColorDepth", "24"),
    ("RdpDefaultBitmapCaching", "false"),
    ("RdpDefaultCompression", "false"),
    ("RdpDefaultHardwareAcceleration", "true"),
    ("RdpDefaultAutoReconnect", "false"),
    ("RdpResolutionPresets", r#"["1280x720"]"#),
    ("MaxEmbeddedSessions", "4"),
    ("SftpBrowserEnabled", "false"),
    ("SftpAutoOpenOnSsh", "false"),
    ("SftpFollowSshDirectory", "true"),
    ("ExternalEditorPath", r#""C:\\Tools\\edit.exe""#),
    ("PreventSleepDuringSession", "false"),
    ("SessionLoggingEnabled", "true"),
    ("SessionLogDirectory", r#""\\\\share\\logs""#),
    ("SessionLogRetentionDays", "90"),
    ("BroadcastScope", r#""AllTabs""#),
    ("CollapseTunnelsPanelByDefault", "false"),
    ("ShowGatewayBadge", "false"),
    ("GroupSessionTabsByFolder", "true"),
    ("UseExternalCredentialProvider", "true"),
    ("CredentialProviderType", "1"),
    ("CredentialProviderCommand", r#""pass show {Title}""#),
    ("CredentialProviderDatabase", r#""D:\\vault.kdbx""#),
    ("CredentialProviderKeyFile", r#""D:\\vault.key""#),
    (
        "CredentialProviderUsernameCommand",
        r#""pass show {Title}/user""#,
    ),
    ("CredentialProviderFirstLineOnly", "true"),
    ("RequireWindowsHelloOnConnect", "true"),
    ("WindowsHelloGraceMinutes", "0"),
    ("PuttyPath", r#""C:\\PuTTY\\putty.exe""#),
    ("X11ServerPath", r#""D:\\X\\vcxsrv.exe""#),
    ("X11AutoStart", "false"),
    ("SessionHealthMonitorEnabled", "false"),
    ("SessionHealthCheckIntervalSeconds", "120"),
    ("SessionHealthProbeTimeoutMs", "500"),
    ("SessionHealthMaxConcurrent", "4"),
    ("VaultHelloMaxDaysBeforeMasterPassword", "30"),
    ("AutoLockIdleMinutes", "10"),
    ("DisconnectOnLock", "true"),
    ("SshAutoReconnectFirstDelaySeconds", "3"),
    ("SshAutoReconnectSecondDelaySeconds", "10"),
    ("SshAutoReconnectSubsequentDelaySeconds", "60"),
    ("RdpConfirmDisconnect", "false"),
    ("RdpRedirectionIndicatorsAlwaysExpanded", "true"),
];

#[test]
fn every_preference_carried_over_is_shown_as_a_change_before_it_is_taken() {
    // Nothing is laid over silently: what the review does not show is not taken.
    for (key, value) in EVERY_PREFERENCE {
        let text = export_file(&format!(r#""{key}": {value}"#));
        let read = read(&text);
        assert_eq!(outcome(&read, key), Some(PreferenceOutcome::Taken), "{key}");
        let review = Settings::default().csharp_import(&read);
        assert!(!review.changes.is_empty(), "{key}: taken but not shown");
        assert_ne!(review.settings, Settings::default(), "{key}");
    }
    let all = EVERY_PREFERENCE
        .iter()
        .map(|(key, value)| format!(r#""{key}": {value}"#))
        .collect::<Vec<_>>()
        .join(", ");
    let read = read(&export_file(&all));
    assert_eq!(read.count(PreferenceOutcome::Taken), EVERY_PREFERENCE.len());
    let mut listed: Vec<&str> = EVERY_PREFERENCE.iter().map(|(key, _)| *key).collect();
    let mut carried = carried_keys();
    listed.sort_unstable();
    carried.sort_unstable();
    assert_eq!(
        listed, carried,
        "every preference carried over is tried here"
    );
    assert_eq!(
        read.count(PreferenceOutcome::NoEquivalent),
        0,
        "{:?}",
        read.rows
    );
}

#[test]
fn the_ssh_reconnect_waits_and_the_rdp_options_are_carried_over() {
    let read = read(&settings_json(
        r#""sshAutoReconnectFirstDelaySeconds": 3, "sshAutoReconnectSecondDelaySeconds": 10,
           "sshAutoReconnectSubsequentDelaySeconds": 60, "rdpConfirmDisconnect": false,
           "rdpRedirectionIndicatorsAlwaysExpanded": true"#,
    ));
    let settings = &read.settings;
    assert_eq!(settings.ssh_auto_reconnect_first_delay, 3);
    assert_eq!(settings.ssh_auto_reconnect_second_delay, 10);
    assert_eq!(settings.ssh_auto_reconnect_subsequent_delay, 60);
    assert!(!settings.rdp_confirm_disconnect);
    assert!(settings.rdp_redirection_indicators_expanded);
    assert_eq!(
        outcome(&read, "SshAutoReconnectFirstDelaySeconds"),
        Some(PreferenceOutcome::Taken)
    );
    assert_eq!(
        outcome(&read, "RdpConfirmDisconnect"),
        Some(PreferenceOutcome::Taken)
    );

    // Out of the C# range (1 to 600), the default is kept.
    let read = read_text_out_of_range();
    assert_eq!(read.settings.ssh_auto_reconnect_first_delay, 2);
    assert_eq!(read.settings.ssh_auto_reconnect_subsequent_delay, 15);
}

fn read_text_out_of_range() -> Preferences {
    read(&settings_json(
        r#""sshAutoReconnectFirstDelaySeconds": 0, "sshAutoReconnectSubsequentDelaySeconds": 601"#,
    ))
}
