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

use std::path::PathBuf;

use heimdall_core::import::csharp::{ImportError, SkipReason, import};
use heimdall_core::profile::{
    DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT,
    DEFAULT_WINRM_HTTP_PORT, DEFAULT_WINRM_HTTPS_PORT, LocalArguments, ProfileId, SshProfile,
};

/// Environment variable naming a directory that holds a real C# `servers.json`, and
/// optionally `settings.json`, for [`a_real_legacy_file_imports_without_error`].
const REAL_DATA_VARIABLE: &str = "HEIMDALL_CS_DATA";

fn servers(entries: &str) -> String {
    format!(r#"{{"schemaVersion": 1, "servers": [{entries}]}}"#)
}

fn settings(group_defaults: &str) -> String {
    format!(r#"{{"groupDefaults": {{{group_defaults}}}}}"#)
}

fn only_profile(servers_json: &str, settings_json: Option<&str>) -> SshProfile {
    let report = import(servers_json, settings_json).expect("valid JSON");
    assert!(report.skipped.is_empty(), "skipped: {:?}", report.skipped);
    assert_eq!(report.profiles.len(), 1);
    report.profiles.into_iter().next().expect("one profile")
}

#[test]
fn an_ssh_profile_is_imported_with_its_own_fields() {
    let json = servers(
        r#"{"id": "a1", "displayName": "Web", "remoteServer": " web.lab ", "connectionType": "SSH",
            "group": "PROD/Linux", "sshUsername": "admin", "sshPort": 2222,
            "sshKeyPath": "C:\\keys\\admin.ppk", "sshPasswordEncrypted": "AQAAANCMnd8BFdERjHoAwE"}"#,
    );
    let profile = only_profile(&json, None);
    assert_eq!(profile.id.as_str(), "a1");
    assert_eq!(profile.name, "Web");
    assert_eq!(profile.group.as_deref(), Some("PROD/Linux"));
    assert_eq!(profile.host, "web.lab");
    assert_eq!(profile.port, 2222);
    assert_eq!(profile.username.as_deref(), Some("admin"));
    assert_eq!(profile.key_path, Some(PathBuf::from("C:\\keys\\admin.ppk")));
}

#[test]
fn an_absent_port_is_the_ssh_default() {
    let json = servers(r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH"}"#);
    assert_eq!(only_profile(&json, None).port, DEFAULT_SSH_PORT);
}

#[test]
fn a_profile_without_a_name_is_named_after_its_host() {
    let json = servers(r#"{"id": "a", "remoteServer": "h.lab", "connectionType": "SSH"}"#);
    assert_eq!(only_profile(&json, None).name, "h.lab");
}

#[test]
fn a_profile_without_connection_type_is_rdp_as_in_the_csharp() {
    let json = servers(r#"{"id": "a", "remoteServer": "h"}"#);
    let report = import(&json, None).expect("valid JSON");
    assert!(report.profiles.is_empty());
    assert_eq!(report.rdp.len(), 1);
    assert_eq!(report.rdp[0].port, DEFAULT_RDP_PORT);
}

#[test]
fn an_rdp_profile_keeps_its_port_user_and_domain() {
    let json = servers(
        r#"{"id": "r", "displayName": "DC", "remoteServer": " dc.lab ", "connectionType": "RDP",
            "remotePort": 3390, "rdpUsername": "admin", "rdpDomain": "LAB", "group": "Win"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let profile = &report.rdp[0];
    assert_eq!(profile.id.as_str(), "r");
    assert_eq!(profile.name, "DC");
    assert_eq!(profile.host, "dc.lab");
    assert_eq!(profile.port, 3390);
    assert_eq!(profile.username.as_deref(), Some("admin"));
    assert_eq!(profile.domain.as_deref(), Some("LAB"));
    assert_eq!(profile.group.as_deref(), Some("Win"));
}

#[test]
fn an_rdp_profile_goes_through_its_gateway_unless_direct() {
    let json = servers(
        r#"{"id": "tunnel", "remoteServer": "h", "connectionType": "RDP", "sshGatewayId": "g"},
           {"id": "direct", "remoteServer": "h", "connectionType": "RDP", "sshGatewayId": "g",
            "useDirectConnection": true},
           {"id": "rdg", "remoteServer": "h", "connectionType": "RDP", "rdpGateway": "rdg.lab"},
           {"id": "rdg-blank", "remoteServer": "h", "connectionType": "RDP", "rdpGateway": " "},
           {"id": "port", "remoteServer": "h", "connectionType": "RDP", "remotePort": 0}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let kept: Vec<&str> = report.rdp.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(kept, ["direct", "rdg-blank"]);
    let reasons: Vec<(String, SkipReason)> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            // Its gateway is not in the settings, which this import has none of.
            ("tunnel".to_owned(), SkipReason::MissingGateway),
            ("rdg".to_owned(), SkipReason::NeedsRdGateway),
            ("port".to_owned(), SkipReason::InvalidPort(0)),
        ]
    );
}

#[test]
fn every_skip_reason_is_reported() {
    let json = servers(
        r#"{"id": "citrix", "remoteServer": "h", "connectionType": "Citrix"},
           {"id": "gw", "remoteServer": "h", "connectionType": "SSH", "sshGatewayId": "g1"},
           {"id": "nohost", "remoteServer": "  ", "connectionType": "SSH"},
           {"id": "", "remoteServer": "h", "connectionType": "SSH"},
           {"id": "port0", "remoteServer": "h", "connectionType": "SSH", "sshPort": 0},
           {"id": "portbig", "remoteServer": "h", "connectionType": "SSH", "sshPort": 70000}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let reasons: Vec<(String, SkipReason)> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert!(report.profiles.is_empty());
    assert_eq!(
        reasons,
        vec![
            ("citrix".to_owned(), SkipReason::NotSsh("Citrix".to_owned())),
            // Its gateway is not in the settings, which this import has none of.
            ("gw".to_owned(), SkipReason::MissingGateway),
            ("nohost".to_owned(), SkipReason::MissingHost),
            (String::new(), SkipReason::MissingId),
            ("port0".to_owned(), SkipReason::InvalidPort(0)),
            ("portbig".to_owned(), SkipReason::InvalidPort(70000)),
        ]
    );
}

#[test]
fn group_defaults_fill_what_the_profile_leaves_unset() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "PROD",
            "sshUsername": ""}"#,
    );
    let defaults = settings(
        r#""PROD": {"sshUsername": "deploy", "sshKeyPath": "/k/deploy", "sshPort": 2200}"#,
    );
    let profile = only_profile(&json, Some(&defaults));
    assert_eq!(profile.username.as_deref(), Some("deploy"));
    assert_eq!(profile.key_path, Some(PathBuf::from("/k/deploy")));
    assert_eq!(profile.port, 2200);
}

#[test]
fn a_profile_value_beats_the_group_default() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "PROD",
            "sshUsername": "mine", "sshPort": 22}"#,
    );
    let defaults = settings(r#""PROD": {"sshUsername": "deploy", "sshPort": 2200}"#);
    let profile = only_profile(&json, Some(&defaults));
    assert_eq!(profile.username.as_deref(), Some("mine"));
    // An explicit port equal to the default still wins: presence, not value, decides.
    assert_eq!(profile.port, 22);
}

#[test]
fn the_deepest_group_wins_field_by_field() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "PROD/Linux/Web"}"#,
    );
    let defaults = settings(
        r#""PROD": {"sshUsername": "root-user", "sshPort": 2201},
           "PROD/Linux": {"sshUsername": "linux-user"}"#,
    );
    let profile = only_profile(&json, Some(&defaults));
    assert_eq!(profile.username.as_deref(), Some("linux-user"));
    assert_eq!(profile.port, 2201);
}

#[test]
fn an_empty_string_in_a_deeper_group_stops_the_search_as_in_the_csharp() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "PROD/Linux"}"#,
    );
    let defaults = settings(
        r#""PROD": {"sshUsername": "root-user"},
           "PROD/Linux": {"sshUsername": ""}"#,
    );
    // `??=` keeps the empty string of PROD/Linux; the profile then receives an empty user
    // name, which the importer stores as no user name.
    assert_eq!(only_profile(&json, Some(&defaults)).username, None);
}

#[test]
fn a_group_default_gateway_applies_to_the_profile() {
    let json =
        servers(r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "G"}"#);
    let defaults = r#"{"groupDefaults": {"G": {"sshGatewayId": "gw-1"}},
        "sshGateways": [{"id": "gw-1", "name": "Bastion", "host": "bastion.lab"}]}"#;
    let profile = only_profile(&json, Some(defaults));
    assert_eq!(profile.gateway, Some(ProfileId::new("gw-1")));
}

#[test]
fn gateways_are_imported_with_their_parents_and_profiles_keep_theirs() {
    let json = servers(
        r#"{"id": "web", "remoteServer": "web.lab", "connectionType": "SSH", "sshGatewayId": "inner"},
           {"id": "desk", "remoteServer": "desk.lab", "connectionType": "RDP", "sshGatewayId": "inner"}"#,
    );
    let gateways = r#"{"sshGateways": [
        {"id": "inner", "name": "Inner", "host": " inner.lab ", "port": 2222, "user": "ops",
         "keyPath": "C:\\keys\\ops", "parentGatewayId": "outer",
         "sshPasswordEncrypted": "AQAAANCMnd8", "hostKeyFingerprint": "SHA256:x"},
        {"id": "outer", "name": "", "host": "outer.example.org", "user": ""}]}"#;
    let report = import(&json, Some(gateways)).expect("valid JSON");
    let [inner, outer] = report.gateways.as_slice() else {
        panic!("{:?}", report.gateways);
    };
    assert_eq!(
        (inner.host.as_str(), inner.port, inner.username.as_deref()),
        ("inner.lab", 2222, Some("ops"))
    );
    assert_eq!(inner.key_path, Some(PathBuf::from(r"C:\keys\ops")));
    assert_eq!(inner.parent, Some(ProfileId::new("outer")));
    assert_eq!(outer.name, "outer.example.org", "named after its host");
    assert_eq!(outer.port, DEFAULT_SSH_PORT);
    assert_eq!(outer.username, None);
    assert_eq!(outer.parent, None);
    assert_eq!(report.profiles[0].gateway, Some(ProfileId::new("inner")));
    assert_eq!(report.rdp[0].gateway, Some(ProfileId::new("inner")));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
}

#[test]
fn a_gateway_whose_parents_are_missing_or_loop_is_left_out_and_so_are_its_profiles() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "a.lab", "connectionType": "SSH", "sshGatewayId": "orphan"},
           {"id": "b", "remoteServer": "b.lab", "connectionType": "SSH", "sshGatewayId": "one"}"#,
    );
    let gateways = r#"{"sshGateways": [
        {"id": "orphan", "host": "o.lab", "parentGatewayId": "nowhere"},
        {"id": "one", "host": "1.lab", "parentGatewayId": "two"},
        {"id": "two", "host": "2.lab", "parentGatewayId": "one"},
        {"id": "", "host": "x.lab"},
        {"id": "nohost", "host": " "}]}"#;
    let report = import(&json, Some(gateways)).expect("valid JSON");
    assert!(report.gateways.is_empty(), "{:?}", report.gateways);
    assert!(report.profiles.is_empty());
    let reasons: Vec<(&str, &SkipReason)> = report
        .skipped
        .iter()
        .map(|skipped| (skipped.id.as_str(), &skipped.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            ("a", &SkipReason::MissingGateway),
            ("b", &SkipReason::MissingGateway),
            ("", &SkipReason::MissingId),
            ("nohost", &SkipReason::MissingHost),
            ("orphan", &SkipReason::MissingGateway),
            ("one", &SkipReason::GatewayLoop),
            ("two", &SkipReason::GatewayLoop),
        ]
    );
}

#[test]
fn a_group_default_connection_type_applies_only_to_an_empty_one() {
    let json = servers(
        r#"{"id": "empty", "remoteServer": "h", "connectionType": "", "group": "G"},
           {"id": "rdp", "remoteServer": "h", "connectionType": "RDP", "group": "G"}"#,
    );
    let defaults = settings(r#""G": {"connectionType": "SSH"}"#);
    let report = import(&json, Some(&defaults)).expect("valid JSON");
    assert_eq!(report.profiles.len(), 1);
    assert_eq!(report.profiles[0].id.as_str(), "empty");
    assert_eq!(report.rdp.len(), 1, "an explicit RDP stays RDP");
    assert_eq!(report.rdp[0].id.as_str(), "rdp");
}

#[test]
fn unknown_fields_are_ignored() {
    let json = servers(
        r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "rdpNla": true,
            "postConnectSteps": [{"input": "ls"}], "futureField": {"x": 1}}"#,
    );
    assert_eq!(only_profile(&json, None).id.as_str(), "a");
}

#[test]
fn malformed_files_are_errors() {
    assert!(matches!(import("{", None), Err(ImportError::Servers(_))));
    assert!(matches!(
        import(&servers(""), Some("[1, 2]")),
        Err(ImportError::Settings(_))
    ));
}

#[test]
fn a_real_legacy_file_imports_without_error() {
    // Runs against real data only when a directory is named; skipped silently otherwise,
    // so CI, which has no such file, is not affected.
    let Some(dir) = std::env::var_os(REAL_DATA_VARIABLE).map(PathBuf::from) else {
        return;
    };
    let servers_json =
        std::fs::read_to_string(dir.join(heimdall_core::paths::LEGACY_SERVERS_FILE_NAME))
            .expect("servers.json is readable");
    let settings_json =
        std::fs::read_to_string(dir.join(heimdall_core::paths::LEGACY_SETTINGS_FILE_NAME)).ok();
    let report = import(&servers_json, settings_json.as_deref()).expect("real file imports");
    println!(
        "imported {} profiles, skipped {}: {:?}",
        report.profiles.len(),
        report.skipped.len(),
        report
            .skipped
            .iter()
            .map(|skipped| &skipped.reason)
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_rdp_profile_without_nla_allows_plain_tls_and_the_default_does_not() {
    let json = servers(
        r#"{"id": "nla", "remoteServer": "h", "connectionType": "RDP"},
           {"id": "tls", "remoteServer": "h", "connectionType": "RDP", "rdpNla": false,
            "rdpUseGlobalDefaults": false},
           {"id": "explicit", "remoteServer": "h", "connectionType": "RDP", "rdpNla": true,
            "rdpUseGlobalDefaults": false}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let allowed: Vec<(&str, bool)> = report
        .rdp
        .iter()
        .map(|profile| (profile.id.as_str(), profile.allow_tls_only))
        .collect();
    assert_eq!(
        allowed,
        [("nla", false), ("tls", true), ("explicit", false)]
    );
}

#[test]
fn a_telnet_profile_is_imported_whatever_the_case_of_its_type() {
    let json = servers(
        r#"{"id": "t", "displayName": "Switch", "remoteServer": " sw1.lab ",
            "connectionType": "Telnet", "telnetPort": 2323, "group": "Network",
            "telnetUsername": "admin", "telnetPasswordEncrypted": "AQAAAN..."},
           {"id": "u", "remoteServer": "sw2.lab", "connectionType": "TELNET"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert!(report.profiles.is_empty() && report.rdp.is_empty());
    let [first, second] = report.telnet.as_slice() else {
        panic!("{:?}", report.telnet);
    };
    assert_eq!(first.id.as_str(), "t");
    assert_eq!(first.name, "Switch");
    assert_eq!(first.host, "sw1.lab");
    assert_eq!(first.port, 2323);
    assert_eq!(first.group.as_deref(), Some("Network"));
    assert_eq!(second.name, "sw2.lab", "named after its host");
    assert_eq!(second.port, DEFAULT_TELNET_PORT);
}

#[test]
fn a_telnet_port_of_zero_or_less_is_the_default_and_too_high_is_left_out() {
    let json = servers(
        r#"{"id": "z", "remoteServer": "h", "connectionType": "Telnet", "telnetPort": 0},
           {"id": "n", "remoteServer": "h", "connectionType": "Telnet", "telnetPort": -5},
           {"id": "x", "remoteServer": "h", "connectionType": "Telnet", "telnetPort": 70000}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let ports: Vec<u16> = report.telnet.iter().map(|profile| profile.port).collect();
    assert_eq!(ports, [DEFAULT_TELNET_PORT, DEFAULT_TELNET_PORT]);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].id, "x");
    assert_eq!(report.skipped[0].reason, SkipReason::InvalidPort(70000));
}

#[test]
fn a_telnet_profile_ignores_the_ssh_gateway_as_the_csharp_does() {
    let json = servers(
        r#"{"id": "t", "remoteServer": "h", "connectionType": "Telnet", "sshGatewayId": "gw"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert_eq!(report.telnet.len(), 1, "{:?}", report.skipped);
}

#[test]
fn a_vnc_profile_keeps_its_port_and_view_only_and_never_its_password() {
    let json = servers(
        r#"{"id": "v", "displayName": "Kiosk", "remoteServer": " kiosk.lab ",
            "connectionType": "VNC", "vncPort": 5901, "vncViewOnly": true,
            "vncPassword": "AQAAANCMnd8BFdERjHoAwE", "group": "Floor"},
           {"id": "w", "remoteServer": "open.lab", "connectionType": "vnc", "vncPort": 0}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let [kiosk, open] = report.vnc.as_slice() else {
        panic!("{:?}", report.vnc);
    };
    assert_eq!(
        (kiosk.name.as_str(), kiosk.host.as_str(), kiosk.port),
        ("Kiosk", "kiosk.lab", 5901)
    );
    assert!(kiosk.view_only);
    assert!(!kiosk.allow_no_password, "it had a password");
    assert_eq!(kiosk.group.as_deref(), Some("Floor"));
    assert_eq!(open.port, DEFAULT_VNC_PORT, "zero is the default port");
    assert!(!open.view_only);
    assert!(open.allow_no_password, "no password in the C# profile");
}

#[test]
fn a_local_profile_keeps_its_argument_string_as_written_and_is_never_approved() {
    let json = servers(
        r#"{"id": "l", "displayName": "Jump", "connectionType": "LOCAL", "group": "Tools",
            "localShellExecutable": " pwsh.exe ",
            "localShellArguments": "-NoExit  -Command \"ssh a\"",
            "localShellWorkingDirectory": " C:\\work ",
            "executionConfirmed": true, "elevationMode": 0},
           {"id": "d", "connectionType": "local", "localShellArguments": "  ",
            "elevationMode": "None"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let [jump, default] = report.local.as_slice() else {
        panic!("{:?}", report.local);
    };
    assert_eq!(jump.name, "Jump");
    assert_eq!(jump.group.as_deref(), Some("Tools"));
    assert_eq!(jump.command.program.as_deref(), Some("pwsh.exe"));
    // Spaces and quotes as the C# handed them over: not split, not re-quoted.
    assert_eq!(
        jump.command.arguments,
        LocalArguments::WindowsLine("-NoExit  -Command \"ssh a\"".to_owned())
    );
    assert_eq!(
        jump.command.working_directory,
        Some(PathBuf::from(r"C:\work"))
    );
    assert_eq!(jump.approved, None, "confirmed in the C#, not here");
    assert!(default.command.is_default(), "{:?}", default.command);
    assert_eq!(default.name, "d");
}

#[test]
fn a_local_profile_asking_for_elevation_is_left_out_whatever_the_form() {
    let json = servers(
        r#"{"id": "auto", "connectionType": "LOCAL", "elevationMode": 1},
           {"id": "runas", "connectionType": "LOCAL", "elevationMode": "runas"},
           {"id": "unknown", "connectionType": "LOCAL", "elevationMode": 7},
           {"id": "odd", "connectionType": "LOCAL", "elevationMode": true},
           {"id": "legacy", "connectionType": "LOCAL", "localShellElevated": true,
            "elevationMode": 0}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.local.is_empty(), "{:?}", report.local);
    assert!(
        report
            .skipped
            .iter()
            .all(|skipped| skipped.reason == SkipReason::NeedsElevation),
        "{:?}",
        report.skipped
    );
    assert_eq!(report.skipped.len(), 5);
}

#[test]
fn a_local_profile_with_commands_to_run_after_start_is_left_out() {
    let json = servers(
        r#"{"id": "steps", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": true, "input": "whoami"}]},
           {"id": "library", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": true, "commandLibraryId": "c1"}]},
           {"id": "off", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": false, "input": "whoami"},
                                 {"enabled": true, "input": "  "}]}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let skipped: Vec<(&str, &SkipReason)> = report
        .skipped
        .iter()
        .map(|skipped| (skipped.id.as_str(), &skipped.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            ("steps", &SkipReason::NeedsPostConnectCommands),
            ("library", &SkipReason::NeedsPostConnectCommands),
        ]
    );
    assert_eq!(report.local.len(), 1, "nothing enabled to run");
}

#[test]
fn a_local_command_that_cannot_be_run_as_written_is_left_out() {
    let json = servers(
        r#"{"id": "quote", "connectionType": "LOCAL",
            "localShellExecutable": "C:\\x\\a.exe\" & calc"},
           {"id": "relative", "connectionType": "LOCAL",
            "localShellExecutable": "tools\\x.exe"},
           {"id": "unc", "connectionType": "LOCAL",
            "localShellWorkingDirectory": "\\\\attacker\\share"},
           {"id": "relative-folder", "connectionType": "LOCAL",
            "localShellWorkingDirectory": "work"},
           {"id": "nul", "connectionType": "LOCAL", "localShellArguments": "/c x\u0000 /p"},
           {"id": "full", "connectionType": "LOCAL",
            "localShellExecutable": "C:\\Program Files\\PowerShell\\7\\pwsh.exe"},
           {"id": "bare", "connectionType": "LOCAL", "localShellExecutable": "cmd.exe"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let skipped: Vec<&str> = report
        .skipped
        .iter()
        .inspect(|skipped| assert_eq!(skipped.reason, SkipReason::UnsafeLocalCommand))
        .map(|skipped| skipped.id.as_str())
        .collect();
    assert_eq!(
        skipped,
        ["quote", "relative", "unc", "relative-folder", "nul"]
    );
    let kept: Vec<&str> = report.local.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(kept, ["full", "bare"]);
}

#[test]
fn the_clipboard_is_shared_unless_the_csharp_profile_turned_it_off() {
    let json = servers(
        r#"{"id": "on", "remoteServer": "h", "connectionType": "RDP"},
           {"id": "off", "remoteServer": "h", "connectionType": "RDP",
            "rdpRedirectClipboard": false, "rdpUseGlobalDefaults": false}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let shared: Vec<(&str, bool)> = report
        .rdp
        .iter()
        .map(|profile| (profile.id.as_str(), profile.redirect_clipboard))
        .collect();
    assert_eq!(shared, [("on", true), ("off", false)]);
}

#[test]
fn drives_are_shared_only_when_the_csharp_profile_shared_them() {
    let json = servers(
        r#"{"id": "off", "remoteServer": "h", "connectionType": "RDP"},
           {"id": "on", "remoteServer": "h", "connectionType": "RDP",
            "rdpRedirectDrives": true, "rdpUseGlobalDefaults": false}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let shared: Vec<(&str, bool)> = report
        .rdp
        .iter()
        .map(|profile| (profile.id.as_str(), profile.redirect_drives))
        .collect();
    assert_eq!(shared, [("off", false), ("on", true)]);
}

#[test]
fn an_rdp_profile_on_the_global_defaults_takes_the_settings_not_its_own_choices() {
    use heimdall_core::profile::{AudioPlayback, ColorDepth};

    // Its own choices say the opposite of the settings: only the settings may show.
    let own = r#""rdpRedirectClipboard": true, "rdpRedirectDrives": false, "rdpNla": true,
                  "rdpColorDepth": 32, "rdpAudioMode": 0, "rdpAdminMode": true"#;
    let json = servers(&format!(
        r#"{{"id": "global", "remoteServer": "h", "connectionType": "RDP", {own}}},
           {{"id": "said", "remoteServer": "h", "connectionType": "RDP", {own},
             "rdpUseGlobalDefaults": true}},
           {{"id": "own", "remoteServer": "h", "connectionType": "RDP", {own},
             "rdpUseGlobalDefaults": false}}"#
    ));
    let settings = r#"{"rdpDefaultRedirectClipboard": false, "rdpDefaultRedirectDrives": true,
                       "rdpDefaultNla": false, "rdpDefaultColorDepth": 16,
                       "rdpDefaultAudioMode": 2}"#;
    let report = import(&json, Some(settings)).expect("valid JSON");
    let taken: Vec<_> = report
        .rdp
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                p.redirect_clipboard,
                p.redirect_drives,
                p.allow_tls_only,
                p.options.color_depth,
                p.options.audio,
                p.options.admin_session,
            )
        })
        .collect();
    let global = (
        false,
        true,
        true,
        ColorDepth::Bpp16,
        AudioPlayback::OnServer,
        true,
    );
    let own = (
        true,
        false,
        false,
        ColorDepth::Bpp32,
        AudioPlayback::Off,
        true,
    );
    let with = |id, v: (bool, bool, bool, ColorDepth, AudioPlayback, bool)| {
        (id, v.0, v.1, v.2, v.3, v.4, v.5)
    };
    assert_eq!(
        taken,
        [
            with("global", global),
            with("said", global),
            with("own", own)
        ],
        "the administrative session is never a global default"
    );
}

#[test]
fn the_global_defaults_left_unset_are_the_csharp_ones() {
    use heimdall_core::profile::RdpOptions;

    let json = servers(r#"{"id": "a", "remoteServer": "h", "connectionType": "RDP"}"#);
    let report = import(&json, Some("{}")).expect("valid JSON");
    let profile = &report.rdp[0];
    assert!(profile.redirect_clipboard);
    assert!(!profile.redirect_drives);
    assert!(!profile.allow_tls_only);
    assert_eq!(profile.options, RdpOptions::default());
}

#[test]
fn a_csharp_colour_depth_is_brought_to_one_the_session_can_have_and_sound_played_here_is_not() {
    use heimdall_core::profile::{AudioPlayback, ColorDepth};

    let entry = |id: &str, depth: i64, audio: i64| {
        format!(
            r#"{{"id": "{id}", "remoteServer": "h", "connectionType": "RDP",
                "rdpUseGlobalDefaults": false, "rdpColorDepth": {depth}, "rdpAudioMode": {audio}}}"#
        )
    };
    let json = servers(
        &[
            entry("8", 8, 0),
            entry("16", 16, 1),
            entry("17", 17, 2),
            entry("24", 24, 3),
            entry("25", 25, 0),
        ]
        .join(","),
    );
    let report = import(&json, None).expect("valid JSON");
    let taken: Vec<_> = report
        .rdp
        .iter()
        .map(|p| (p.id.as_str(), p.options.color_depth, p.options.audio))
        .collect();
    assert_eq!(
        taken,
        [
            ("8", ColorDepth::Bpp16, AudioPlayback::Off),
            // Played on this computer: this version does not, so it is not played.
            ("16", ColorDepth::Bpp16, AudioPlayback::Off),
            ("17", ColorDepth::Bpp24, AudioPlayback::OnServer),
            ("24", ColorDepth::Bpp24, AudioPlayback::Off),
            ("25", ColorDepth::Bpp32, AudioPlayback::Off),
        ]
    );
}

#[test]
fn a_winrm_profile_keeps_its_transport_and_account() {
    let json = servers(
        r#"{"id": "w", "displayName": "DC", "remoteServer": " dc.lab ", "connectionType": "WinRM",
            "group": "Win", "winRmPort": 5999, "winRmUseSsl": true,
            "winRmSkipCertificateCheck": true, "winRmIdentityMode": "Credential",
            "winRmUsername": " LAB\\admin ", "winRmPasswordEncrypted": "AQAAANCMnd8BFdERjHoAwE"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert!(report.profiles.is_empty() && report.rdp.is_empty());
    let profile = &report.winrm[0];
    assert_eq!(profile.id.as_str(), "w");
    assert_eq!(profile.name, "DC");
    assert_eq!(profile.group.as_deref(), Some("Win"));
    assert_eq!(profile.host, "dc.lab");
    assert_eq!(profile.port, 5999);
    assert!(profile.use_ssl);
    assert!(profile.skip_certificate_check);
    assert_eq!(profile.username.as_deref(), Some("LAB\\admin"));
}

#[test]
fn a_winrm_port_left_unset_is_the_default_of_its_transport() {
    let json = servers(
        r#"{"id": "http", "remoteServer": "h", "connectionType": "WINRM"},
           {"id": "https", "remoteServer": "h", "connectionType": "WINRM", "winRmUseSsl": true},
           {"id": "zero", "remoteServer": "h", "connectionType": "WINRM", "winRmPort": 0,
            "winRmUseSsl": true}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let ports: Vec<(&str, u16)> = report
        .winrm
        .iter()
        .map(|p| (p.id.as_str(), p.port))
        .collect();
    assert_eq!(
        ports,
        [
            ("http", DEFAULT_WINRM_HTTP_PORT),
            ("https", DEFAULT_WINRM_HTTPS_PORT),
            ("zero", DEFAULT_WINRM_HTTPS_PORT)
        ]
    );
}

#[test]
fn a_winrm_profile_as_the_current_user_names_no_account() {
    let json = servers(
        r#"{"id": "absent", "remoteServer": "h", "connectionType": "WINRM", "winRmUsername": "x"},
           {"id": "name", "remoteServer": "h", "connectionType": "WINRM", "winRmUsername": "x",
            "winRmIdentityMode": "currentuser"},
           {"id": "number", "remoteServer": "h", "connectionType": "WINRM", "winRmUsername": "x",
            "winRmIdentityMode": 0},
           {"id": "credential", "remoteServer": "h", "connectionType": "WINRM",
            "winRmUsername": "x", "winRmIdentityMode": 1}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let accounts: Vec<(&str, Option<&str>)> = report
        .winrm
        .iter()
        .map(|p| (p.id.as_str(), p.username.as_deref()))
        .collect();
    assert_eq!(
        accounts,
        [
            ("absent", None),
            ("name", None),
            ("number", None),
            ("credential", Some("x"))
        ]
    );
}

#[test]
fn skipping_the_certificate_check_needs_https() {
    let json = servers(
        r#"{"id": "w", "remoteServer": "h", "connectionType": "WINRM",
            "winRmSkipCertificateCheck": true}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(!report.winrm[0].skip_certificate_check);
}

#[test]
fn a_winrm_profile_that_cannot_be_connected_is_skipped_with_its_reason() {
    let json = servers(
        r#"{"id": "no-user", "remoteServer": "h", "connectionType": "WINRM",
            "winRmIdentityMode": "Credential", "winRmUsername": "  "},
           {"id": "mode", "remoteServer": "h", "connectionType": "WINRM", "winRmIdentityMode": 7},
           {"id": "mode-name", "remoteServer": "h", "connectionType": "WINRM",
            "winRmIdentityMode": "Kerberos"},
           {"id": "port", "remoteServer": "h", "connectionType": "WINRM", "winRmPort": 70000},
           {"id": "host", "remoteServer": " ", "connectionType": "WINRM"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.winrm.is_empty(), "{:?}", report.winrm);
    let reasons: Vec<(String, SkipReason)> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("no-user".to_owned(), SkipReason::MissingUsername),
            ("mode".to_owned(), SkipReason::UnknownIdentityMode),
            ("mode-name".to_owned(), SkipReason::UnknownIdentityMode),
            ("port".to_owned(), SkipReason::InvalidPort(70000)),
            ("host".to_owned(), SkipReason::MissingHost),
        ]
    );
}
