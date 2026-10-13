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

//! The migration from the legacy PowerShell Heimdall, `RDPManager`.

use std::fs;
use std::path::Path;

use heimdall_core::import::csharp::SkipReason;
use heimdall_core::import::rdpmanager::{
    ConversionError, LEGACY_APP_FOLDER_NAME, LeftOut, LeftOutBecause, MAX_IDENTITY_LENGTH, convert,
    find_installation, is_installation, servers_file, settings_file,
};
use heimdall_core::profile::{AudioPlayback, ColorDepth, ProfileId, RdpMode, SshMode};
use heimdall_core::settings::{
    ANTI_IDLE_INTERVAL_DEFAULT, AppTheme, Language, RDP_CONNECT_TIMEOUT_DEFAULT, Settings,
};

/// A legacy installation at `folder`, with both files.
fn installed(folder: &Path) {
    fs::create_dir_all(settings_file(folder).parent().expect("config")).expect("config");
    fs::write(settings_file(folder), "{}").expect("settings");
    fs::write(servers_file(folder), "[]").expect("servers");
}

#[test]
fn the_installation_is_found_walking_up_from_the_program_folder() {
    let root = tempfile::tempdir().expect("dir");
    let start = root.path().join("apps").join("heimdall").join("bin");
    fs::create_dir_all(&start).expect("start");
    assert_eq!(find_installation(&start), None, "none anywhere");

    // Two levels up, beside the folder's grandparent.
    let high = root.path().join(LEGACY_APP_FOLDER_NAME);
    installed(&high);
    assert_eq!(find_installation(&start), Some(high.clone()));

    // Nearer: the parent's own wins over a farther one.
    let near = root
        .path()
        .join("apps")
        .join("heimdall")
        .join(LEGACY_APP_FOLDER_NAME);
    installed(&near);
    assert_eq!(find_installation(&start), Some(near.clone()));

    // Inside the folder itself: the parent's is looked at first, as the C# does.
    let inside = start.join(LEGACY_APP_FOLDER_NAME);
    installed(&inside);
    assert_eq!(find_installation(&start), Some(near));
    fs::remove_dir_all(
        root.path()
            .join("apps")
            .join("heimdall")
            .join(LEGACY_APP_FOLDER_NAME),
    )
    .expect("removed");
    assert_eq!(find_installation(&start), Some(inside));
}

#[test]
fn an_installation_needs_both_files() {
    let root = tempfile::tempdir().expect("dir");
    let folder = root.path().join(LEGACY_APP_FOLDER_NAME);
    fs::create_dir_all(folder.join("config")).expect("config");
    fs::write(settings_file(&folder), "{}").expect("settings");
    assert!(!is_installation(&folder), "servers.json missing");
    fs::write(servers_file(&folder), "[]").expect("servers");
    assert!(is_installation(&folder));
    fs::remove_file(settings_file(&folder)).expect("removed");
    // A folder in place of a file is no installation.
    fs::create_dir_all(settings_file(&folder)).expect("folder");
    assert!(!is_installation(&folder));
    let start = folder.join("bin");
    fs::create_dir_all(&start).expect("start");
    assert_eq!(find_installation(&start), None);
}

const SERVERS: &str = r#"[
  {
    "Id": "srv-001",
    "DisplayName": "Test Box",
    "RemoteServer": "10.0.0.1",
    "RemotePort": 3389,
    "ConnectionType": "RDP",
    "RdpUsername": "admin",
    "RdpPasswordEncrypted": "DPAPI-RDP-SECRET",
    "IsFavorite": true
  },
  {
    "Id": "srv-002",
    "DisplayName": "SSH Box",
    "RemoteServer": "10.0.0.2",
    "SshPort": 2222,
    "SshUsername": "root",
    "SshKeyPath": "C:\\keys\\id_ed25519",
    "SshPasswordEncrypted": "DPAPI-SSH-SECRET",
    "SshKeyPassphraseEncrypted": "DPAPI-PASSPHRASE-SECRET",
    "SshGatewayId": "gw-1",
    "ConnectionType": "SSH"
  },
  {
    "Id": "rejected-profile",
    "DisplayName": "Rejected\r\nprofile XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "RemoteServer": "rejected.example.test",
    "RemotePort": 999999999999999999999999999999,
    "ConnectionType": "SSH"
  },
  "not a server",
  {
    "Id": "no-host",
    "DisplayName": "No host",
    "ConnectionType": "SSH"
  },
  {
    "Id": "srv-006",
    "DisplayName": "Odd kinds",
    "RemoteServer": "10.0.0.6",
    "ConnectionType": "SSH",
    "SshUsername": 42,
    "IsFavorite": "yes"
  }
]"#;

const SETTINGS: &str = r#"{
  "DefaultLocale": "fr-FR",
  "DefaultTheme": "Tarn",
  "EnableLogging": false,
  "RequireCredentialGuard": true,
  "SshDefaultMode": "External",
  "AntiIdleIntervalSeconds": 3,
  "SshTmoutResetIntervalSeconds": 120,
  "RdpDefaultMode": "External",
  "RdpDefaultRedirectDrives": true,
  "RdpDefaultAudioMode": 1,
  "RdpDefaultColorDepth": 16,
  "RdpDefaultNla": false,
  "MaxEmbeddedSessions": 5,
  "EmbeddedRdpTimeoutMs": 30000,
  "SftpAutoOpenOnSsh": false,
  "PreventSleepDuringSession": false,
  "SessionLoggingEnabled": true,
  "SessionLogDirectory": "\\\\attacker\\share\\logs",
  "LogFilePath": "\\\\attacker\\share\\heimdall.log",
  "PuttyPath": "C:\\RDPManager\\evil\\putty.exe",
  "PlinkPath": "C:\\RDPManager\\evil\\plink.exe",
  "PsftpPath": "C:\\RDPManager\\evil\\psftp.exe",
  "ExternalEditorPath": "C:\\RDPManager\\evil\\editor.exe",
  "PinHash": "LEGACY-PIN-HASH",
  "PinSalt": "LEGACY-PIN-SALT",
  "HmacKey": "LEGACY-HMAC-KEY",
  "LastDpapiUser": "LEGACY-DPAPI-USER",
  "TrustedHostKeys": { "10.0.0.2:2222": "SHA256:PLANTED-TRUST" },
  "SshGateways": [
    {
      "Id": "gw-1",
      "Name": "Bastion",
      "Host": "bastion.example.test",
      "Port": 22,
      "User": "jump",
      "SshPasswordEncrypted": "DPAPI-GATEWAY-SECRET",
      "SshKeyPassphraseEncrypted": "DPAPI-GATEWAY-PASSPHRASE",
      "HostKeyFingerprint": "SHA256:PLANTED-GATEWAY-TRUST"
    },
    { "Id": "", "Name": "No identifier", "Host": "nowhere.example.test" }
  ],
  "Projects": [
    { "Id": "p1", "Name": "Alpha" },
    { "Id": "p2", "Name": "Beta" }
  ]
}"#;

#[test]
fn the_servers_are_mapped_and_each_profile_left_out_is_reported_with_its_place() {
    let conversion = convert(SETTINGS, SERVERS).expect("converted");
    assert_eq!(conversion.examined, 6);
    assert_eq!(conversion.imported(), 3);
    assert_eq!(conversion.gateways, 2);
    assert_eq!(conversion.projects, 2, "projects counted, none kept");

    let rdp = &conversion.report.rdp;
    assert_eq!(rdp.len(), 1);
    assert_eq!(rdp[0].id.as_str(), "srv-001");
    assert_eq!(rdp[0].name, "Test Box");
    assert_eq!(rdp[0].username.as_deref(), Some("admin"));
    // On the global defaults, the legacy settings' RDP choices apply.
    assert!(rdp[0].redirect_drives);
    assert_eq!(
        conversion
            .report
            .favorites
            .iter()
            .map(ProfileId::as_str)
            .collect::<Vec<_>>(),
        ["srv-001"]
    );

    let ssh = &conversion.report.profiles;
    assert_eq!(ssh.len(), 2);
    assert_eq!(ssh[0].id.as_str(), "srv-002");
    assert_eq!(ssh[0].port, 2222);
    assert_eq!(ssh[0].username.as_deref(), Some("root"));
    assert_eq!(ssh[0].gateway.as_ref().map(ProfileId::as_str), Some("gw-1"));
    // A value of the wrong kind is ignored, as the C# `MapString` and `MapBool` do.
    assert_eq!(ssh[1].id.as_str(), "srv-006");
    assert_eq!(ssh[1].username, None);

    assert_eq!(
        conversion.left_out,
        [
            LeftOut {
                index: 3,
                name: Some(format!(
                    "Rejected profile {}",
                    "X".repeat(MAX_IDENTITY_LENGTH - "Rejected profile ".len())
                )),
                reason: LeftOutBecause::InvalidLegacyField,
            },
            LeftOut {
                index: 4,
                name: None,
                reason: LeftOutBecause::InvalidLegacyField,
            },
            LeftOut {
                index: 5,
                name: Some("No host".to_owned()),
                reason: LeftOutBecause::Refused(SkipReason::MissingHost),
            },
        ]
    );
    let long = conversion.left_out[0].name.as_deref().expect("named");
    assert_eq!(long.len(), MAX_IDENTITY_LENGTH);
    assert!(!long.contains(['\r', '\n']));

    // The gateway without an identifier is left out as an imported file's would be.
    assert_eq!(conversion.report.gateways.len(), 1);
    assert_eq!(conversion.report.gateways[0].host, "bastion.example.test");
    assert_eq!(conversion.report.skipped.len(), 1);
    assert_eq!(conversion.report.skipped[0].reason, SkipReason::MissingId);
    assert_eq!(conversion.report.skipped[0].position, None);
}

#[test]
fn no_secret_program_path_log_path_or_trust_is_ever_taken() {
    let conversion = convert(SETTINGS, SERVERS).expect("converted");
    assert!(
        conversion.report.host_keys.is_empty(),
        "no host key from the folder"
    );
    let shown = format!("{conversion:?}");
    for planted in [
        "DPAPI-",
        "LEGACY-",
        "PLANTED",
        "attacker",
        "evil",
        "putty.exe",
        "plink",
        "psftp",
    ] {
        assert!(!shown.contains(planted), "{planted} in {shown}");
    }

    let mut settings = Settings::default();
    conversion.settings.apply_to(&mut settings);
    let defaults = Settings::default();
    assert_eq!(settings.putty_path, defaults.putty_path);
    assert_eq!(settings.external_editor, defaults.external_editor);
    assert_eq!(settings.x11_server_path, defaults.x11_server_path);
    assert_eq!(
        settings.session_log_directory,
        defaults.session_log_directory
    );
    assert_eq!(settings.pin, None);
    assert_eq!(settings.credential_provider, defaults.credential_provider);
}

#[test]
fn the_settings_heimdall_has_are_taken_and_a_value_out_of_range_is_the_default() {
    let conversion = convert(SETTINGS, "[]").expect("converted");
    let mut settings = Settings::default();
    conversion.settings.apply_to(&mut settings);
    assert_eq!(settings.language, Some(Language::French));
    assert_eq!(settings.theme, AppTheme::Tarn);
    assert!(!settings.diagnostics_log);
    assert!(settings.require_credential_guard);
    assert_eq!(settings.ssh_default_mode, SshMode::External);
    assert_eq!(
        settings.anti_idle_interval, ANTI_IDLE_INTERVAL_DEFAULT,
        "3 s is out of the range"
    );
    assert_eq!(settings.ssh_tmout_reset_interval, 120);
    assert_eq!(settings.rdp_default_mode, RdpMode::External);
    assert!(settings.rdp_defaults.redirect_drives);
    assert_eq!(settings.rdp_defaults.audio, AudioPlayback::Local);
    assert_eq!(settings.rdp_defaults.color_depth, ColorDepth::Bpp16);
    assert!(!settings.rdp_defaults.nla);
    assert_eq!(settings.max_sessions, 5);
    assert_eq!(settings.rdp_connect_timeout, 30);
    assert!(!settings.sftp_browser.auto_open_on_ssh);
    assert!(settings.sftp_browser.enabled, "absent: left as it is");
    assert!(!settings.prevent_sleep);
    assert!(settings.session_logging);

    // A protection is never lowered, an unknown name is the default, a timeout out of range
    // the default.
    let mut guarded = Settings {
        require_credential_guard: true,
        theme: AppTheme::Tarn,
        ..Settings::default()
    };
    convert(
        r#"{"RequireCredentialGuard": false, "DefaultTheme": "Nonexistent",
            "EmbeddedRdpTimeoutMs": 1, "MaxEmbeddedSessions": -4, "DefaultLocale": "de-DE"}"#,
        "[]",
    )
    .expect("converted")
    .settings
    .apply_to(&mut guarded);
    assert!(guarded.require_credential_guard);
    assert_eq!(guarded.theme, AppTheme::default());
    assert_eq!(guarded.rdp_connect_timeout, RDP_CONNECT_TIMEOUT_DEFAULT);
    assert_eq!(guarded.max_sessions, Settings::default().max_sessions);
    assert_eq!(guarded.language, None, "a language not offered is left");
}

#[test]
fn a_single_server_written_alone_is_one_and_nothing_written_is_none() {
    let one = convert(
        "{}",
        r#"{ "Id": "solo", "DisplayName": "Solo", "RemoteServer": "10.0.0.9", "ConnectionType": "SSH" }"#,
    )
    .expect("converted");
    assert_eq!(one.examined, 1);
    assert_eq!(one.report.profiles.len(), 1);
    assert_eq!(one.report.profiles[0].id.as_str(), "solo");
    for empty in ["", "  \n", "null", "[]"] {
        let none = convert("{}", empty).expect("converted");
        assert_eq!(none.examined, 0, "{empty:?}");
        assert!(none.left_out.is_empty());
    }
}

#[test]
fn files_of_another_shape_are_refused_whole() {
    assert!(matches!(
        convert("[]", "[]"),
        Err(ConversionError::Settings(_))
    ));
    assert!(matches!(
        convert("not json", "[]"),
        Err(ConversionError::Settings(_))
    ));
    assert!(matches!(
        convert("{}", "42"),
        Err(ConversionError::Servers(_))
    ));
    assert!(matches!(
        convert("{}", "{ broken"),
        Err(ConversionError::Servers(_))
    ));
    // A gateway the C# could not read fails its migration whole.
    assert!(matches!(
        convert(r#"{"SshGateways": [{"Id": "gw", "Port": 1.5}]}"#, "[]"),
        Err(ConversionError::Gateway(1))
    ));
}

/// A key file on a drive of this computer, as this system writes one.
const LOCAL_KEY: &str = if cfg!(windows) {
    "C:\\Users\\me\\.ssh\\id_ed25519"
} else {
    "/home/me/.ssh/id_ed25519"
};

#[test]
fn only_a_plain_local_path_is_a_key_path_and_the_others_are_left_out_and_counted() {
    use heimdall_core::import::rdpmanager::is_plain_local_path;

    assert!(is_plain_local_path(LOCAL_KEY));
    for planted in [
        "\\\\attacker\\share\\id",
        "//attacker/share/id",
        "\\\\?\\C:\\keys\\id",
        "\\\\?\\UNC\\attacker\\share\\id",
        "\\\\.\\pipe\\agent",
        "//./pipe/agent",
        "\\??\\C:\\keys\\id",
        "keys\\id",
        "keys/id",
        "\\keys\\id",
        "C:keys\\id",
        "C:\\keys\\id:stream",
        "/home/me/id\0x",
    ] {
        assert!(!is_plain_local_path(planted), "{planted:?}");
    }

    let server = |id: &str, host: &str, key: &str| {
        serde_json::json!({
            "Id": id, "DisplayName": id, "RemoteServer": host, "ConnectionType": "SSH",
            "SshKeyPath": key, "SshGatewayId": "gw-1"
        })
    };
    let servers = serde_json::Value::Array(vec![
        server("local", "10.0.0.1", LOCAL_KEY),
        server("unc", "10.0.0.2", "\\\\attacker\\share\\id"),
        server("slashes", "10.0.0.3", "//attacker/share/id"),
        server("verbatim", "10.0.0.4", "\\\\?\\C:\\keys\\id"),
        server("device", "10.0.0.5", "\\\\.\\pipe\\agent"),
        server("relative", "10.0.0.6", "keys\\id"),
        server("blank", "10.0.0.7", "  "),
        // Refused for its missing host: its key path is not counted.
        server("no-host", "", "\\\\attacker\\share\\id"),
    ]);
    let settings = serde_json::json!({
        "SshGateways": [
            { "Id": "gw-1", "Host": "bastion", "KeyPath": "\\\\attacker\\share\\gw" },
            { "Id": "gw-2", "Host": "jump", "KeyPath": LOCAL_KEY },
            // Refused for its missing identifier: not counted.
            { "Id": "", "Host": "nowhere", "KeyPath": "\\\\attacker\\share\\x" }
        ]
    });
    let conversion = convert(&settings.to_string(), &servers.to_string()).expect("converted");
    assert_eq!(
        conversion.key_paths_left_out, 6,
        "five profiles and a gateway"
    );
    let profiles = &conversion.report.profiles;
    assert_eq!(profiles.len(), 7, "each imported without its key path");
    for profile in profiles {
        let expected = (profile.id.as_str() == "local").then(|| Path::new(LOCAL_KEY));
        assert_eq!(
            profile.key_path.as_deref(),
            expected,
            "{}",
            profile.id.as_str()
        );
    }
    let gateways = &conversion.report.gateways;
    assert_eq!(gateways.len(), 2);
    assert_eq!(gateways[0].key_path, None, "the planted one left out");
    assert_eq!(gateways[1].key_path.as_deref(), Some(Path::new(LOCAL_KEY)));
    assert!(!format!("{conversion:?}").contains("attacker"));
}
