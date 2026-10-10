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
use heimdall_core::post_connect::{OnFailure, PostConnect, PostConnectStep};
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
    let kept: Vec<(&str, Option<&str>, Option<&str>)> = report
        .rdp
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                p.gateway.as_ref().map(ProfileId::as_str),
                p.extras.rd_gateway(),
            )
        })
        .collect();
    assert_eq!(
        kept,
        [
            // Its gateway is not in the file: imported naming it all the same, as the C#.
            ("tunnel", Some("g"), None),
            ("direct", None, None),
            ("rdg", None, Some("rdg.lab")),
            ("rdg-blank", None, None)
        ],
        "a Remote Desktop Gateway is kept, for the client that goes through it"
    );
    let reasons: Vec<(String, SkipReason)> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![("port".to_owned(), SkipReason::InvalidPort(0))]
    );
}

#[test]
fn every_skip_reason_is_reported() {
    let json = servers(
        r#"{"id": "serial", "remoteServer": "h", "connectionType": "Serial"},
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
    let imported: Vec<&str> = report.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        imported,
        ["gw"],
        "a gateway the file does not hold is no reason to leave a profile out"
    );
    assert_eq!(
        reasons,
        vec![
            ("serial".to_owned(), SkipReason::NotSsh("Serial".to_owned())),
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
fn a_gateway_whose_parents_are_missing_or_loop_is_kept_with_its_profiles_for_the_reconciler() {
    use heimdall_core::import::gateways::{Reconciliation, reconcile};

    let json = servers(
        r#"{"id": "a", "remoteServer": "a.lab", "connectionType": "SSH", "sshGatewayId": "orphan"},
           {"id": "b", "remoteServer": "b.lab", "connectionType": "SSH", "sshGatewayId": "one"},
           {"id": "c", "remoteServer": "c.lab", "connectionType": "SSH", "sshGatewayId": "nohost"}"#,
    );
    let gateways = r#"{"sshGateways": [
        {"id": "orphan", "host": "o.lab", "parentGatewayId": "nowhere"},
        {"id": "one", "host": "1.lab", "parentGatewayId": "two"},
        {"id": "two", "host": "2.lab", "parentGatewayId": "one"},
        {"id": "", "host": "x.lab"},
        {"id": "nohost", "host": " "}]}"#;
    let mut report = import(&json, Some(gateways)).expect("valid JSON");
    let reasons: Vec<(&str, &SkipReason)> = report
        .skipped
        .iter()
        .map(|skipped| (skipped.id.as_str(), &skipped.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            ("", &SkipReason::MissingId),
            ("nohost", &SkipReason::MissingHost)
        ],
        "only a gateway that cannot be used is left out"
    );

    // As the C# `GatewayImportReconciler`: every profile imported, a missing parent and the
    // parent closing the loop cleared, the profile naming a gateway left out keeping it.
    let counts = reconcile(&mut report, &[], &mut || ProfileId::new("fresh"));
    assert_eq!(
        counts,
        Reconciliation {
            created: 3,
            merged: 0,
            orphans: 3
        }
    );
    let parents: Vec<(&str, Option<&str>)> = report
        .gateways
        .iter()
        .map(|g| (g.id.as_str(), g.parent.as_ref().map(ProfileId::as_str)))
        .collect();
    assert_eq!(
        parents,
        [("orphan", None), ("one", None), ("two", Some("one"))],
        "the first gateway of the loop loses its parent, as the C# BreakParentLoops"
    );
    let through: Vec<(&str, Option<&str>)> = report
        .profiles
        .iter()
        .map(|p| (p.id.as_str(), p.gateway.as_ref().map(ProfileId::as_str)))
        .collect();
    assert_eq!(
        through,
        [
            ("a", Some("orphan")),
            ("b", Some("one")),
            ("c", Some("nohost"))
        ],
        "the reference the import cannot resolve kept, as the C# keeps it"
    );
}

#[test]
fn a_profile_naming_a_gateway_already_saved_resolves_to_it_and_is_no_orphan() {
    use heimdall_core::import::gateways::reconcile;
    use heimdall_core::profile::SshGateway;

    let json = servers(
        r#"{"id": "a", "remoteServer": "a.lab", "connectionType": "SSH", "sshGatewayId": "saved"},
           {"id": "w", "remoteServer": "w.lab", "connectionType": "WINRM", "sshGatewayId": "gone"}"#,
    );
    let mut report = import(&json, None).expect("valid JSON");
    let saved = SshGateway {
        id: ProfileId::new("saved"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: DEFAULT_SSH_PORT,
        username: None,
        key_path: None,
        parent: None,
    };
    let counts = reconcile(&mut report, &[saved], &mut || ProfileId::new("fresh"));
    assert_eq!(counts.orphans, 1, "the WinRM profile's gateway alone");
    assert_eq!(report.profiles[0].gateway, Some(ProfileId::new("saved")));
    assert_eq!(report.winrm[0].gateway, Some(ProfileId::new("gone")));
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
fn every_elevation_mode_of_a_local_profile_runs_as_administrator_in_its_own_window() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings, LegacyElevation};

    let json = servers(
        r#"{"id": "auto", "displayName": "Auto", "connectionType": "LOCAL", "elevationMode": 1},
           {"id": "gsudo", "displayName": "Gsudo", "connectionType": "LOCAL",
            "elevationMode": 2},
           {"id": "runas", "displayName": "Runas", "connectionType": "LOCAL",
            "elevationMode": 3},
           {"id": "named", "displayName": "Named", "connectionType": "LOCAL",
            "elevationMode": "gsudo"},
           {"id": "unknown", "displayName": "Unknown", "connectionType": "LOCAL",
            "elevationMode": 7},
           {"id": "odd", "displayName": "Odd", "connectionType": "LOCAL", "elevationMode": true},
           {"id": "legacy", "displayName": "Legacy", "connectionType": "LOCAL",
            "localShellElevated": true, "elevationMode": 0},
           {"id": "none", "displayName": "None", "connectionType": "LOCAL",
            "elevationMode": "None"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let elevated: Vec<(&str, bool)> = report
        .local
        .iter()
        .map(|local| (local.id.as_str(), local.command.run_as_administrator))
        .collect();
    assert_eq!(
        elevated,
        [
            ("auto", true),
            ("gsudo", true),
            ("runas", true),
            ("named", true),
            ("unknown", true),
            ("odd", true),
            ("legacy", true),
            ("none", false),
        ]
    );
    assert!(report.local.iter().all(|local| local.approved.is_none()));
    // One line for each profile whose mode was mapped, naming the C# mode.
    let mapped = |name: &str, mode| DroppedSettings {
        name: name.to_owned(),
        settings: vec![Dropped::Elevation(mode)],
    };
    assert_eq!(
        report.dropped,
        [
            mapped("Auto", LegacyElevation::Auto),
            mapped("Gsudo", LegacyElevation::Gsudo),
            mapped("Runas", LegacyElevation::Runas),
            mapped("Named", LegacyElevation::Gsudo),
            mapped("Unknown", LegacyElevation::Unknown),
            mapped("Odd", LegacyElevation::Unknown),
            // The old box, read as the C# `EffectiveElevationMode` does.
            mapped("Legacy", LegacyElevation::Auto),
        ]
    );
}

#[test]
fn an_elevated_local_profile_says_its_elevation_and_its_dead_steps_on_one_line() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings, LegacyElevation};

    let json = servers(
        r#"{"id": "both", "displayName": "Both", "connectionType": "LOCAL",
            "elevationMode": "Runas", "postConnectCommand": "whoami"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert_eq!(
        report.dropped,
        [DroppedSettings {
            name: "Both".to_owned(),
            settings: vec![
                Dropped::Elevation(LegacyElevation::Runas),
                Dropped::LocalPostConnect(1),
            ],
        }]
    );
}

#[test]
fn a_local_profile_is_imported_without_the_commands_the_csharp_never_ran_after_start() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};

    let json = servers(
        r#"{"id": "steps", "displayName": "Steps", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": true, "input": "whoami"},
                                 {"enabled": true, "input": "hostname"}]},
           {"id": "library", "displayName": "Library", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": true, "commandLibraryId": "c1"}]},
           {"id": "unsaid", "displayName": "Unsaid", "connectionType": "LOCAL",
            "postConnectSteps": [{"input": "whoami"}]},
           {"id": "legacy", "displayName": "Legacy", "connectionType": "LOCAL",
            "postConnectCommand": "whoami\n\n  \nhostname\n"},
           {"id": "off", "displayName": "Off", "connectionType": "LOCAL",
            "postConnectSteps": [{"enabled": false, "input": "whoami"},
                                 {"enabled": true, "input": "  "}]}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let imported: Vec<&str> = report.local.iter().map(|local| local.id.as_str()).collect();
    assert_eq!(imported, ["steps", "library", "unsaid", "legacy", "off"]);
    let dropped = |name: &str, steps| DroppedSettings {
        name: name.to_owned(),
        settings: vec![Dropped::LocalPostConnect(steps)],
    };
    assert_eq!(
        report.dropped,
        [
            dropped("Steps", 2),
            dropped("Library", 1),
            // A step says nothing of being on: on, as a new C# step is.
            dropped("Unsaid", 1),
            // The sequence before steps: one per line written.
            dropped("Legacy", 2),
        ],
        "nothing enabled to run in Off: nothing said"
    );
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
fn auto_reconnect_is_on_unless_the_profile_or_the_defaults_it_follows_clear_it() {
    let json = servers(
        r#"{"id": "unset", "remoteServer": "h", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false},
           {"id": "cleared", "remoteServer": "h", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false, "rdpAutoReconnect": false},
           {"id": "global", "remoteServer": "h", "connectionType": "RDP",
            "rdpAutoReconnect": true}"#,
    );
    let read = |settings| {
        import(&json, Some(settings))
            .expect("valid JSON")
            .rdp
            .iter()
            .map(|profile| (profile.id.as_str().to_owned(), profile.auto_reconnect))
            .collect::<Vec<_>>()
    };
    let owned = |list: [(&str, bool); 3]| list.map(|(id, on)| (id.to_owned(), on)).to_vec();
    assert_eq!(
        read("{}"),
        owned([("unset", true), ("cleared", false), ("global", true)])
    );
    assert_eq!(
        read(r#"{"rdpDefaultAutoReconnect": false}"#),
        owned([("unset", true), ("cleared", false), ("global", false)]),
        "a profile on the defaults takes theirs, not its own"
    );
}

#[test]
fn the_performance_flags_are_the_profile_s_own_even_on_the_global_defaults() {
    let json = servers(
        r#"{"id": "own", "remoteServer": "h", "connectionType": "RDP",
            "rdpPerformanceFlags": 385},
           {"id": "global", "remoteServer": "h", "connectionType": "RDP",
            "rdpUseGlobalDefaults": true, "rdpPerformanceFlags": 1},
           {"id": "none", "remoteServer": "h", "connectionType": "RDP"},
           {"id": "odd", "remoteServer": "h", "connectionType": "RDP",
            "rdpPerformanceFlags": -1}"#,
    );
    let report = import(&json, Some("{}")).expect("valid JSON");
    let flags: Vec<_> = report
        .rdp
        .iter()
        .map(|profile| (profile.id.as_str(), profile.options.performance_flags))
        .collect();
    assert_eq!(
        flags,
        [("own", 0x181), ("global", 0x01), ("none", 0), ("odd", 0)],
        "kept whole; a negative value, which no C# dialog writes, as none"
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
fn a_csharp_resolution_mode_is_read_as_its_embedded_session_sizes_the_desktop() {
    use heimdall_core::profile::Resolution;

    let entry = |id: &str, extra: &str| {
        format!(r#"{{"id": "{id}", "remoteServer": "h", "connectionType": "RDP"{extra}}}"#)
    };
    let json = servers(
        &[
            entry("absent", ""),
            entry(
                "fixed",
                r#", "rdpResolutionMode": "Fixed", "rdpFixedResolutionWidth": 1366,
                   "rdpFixedResolutionHeight": 768, "rdpInitialSmartSizing": false"#,
            ),
            entry(
                "sized-without-mode",
                r#", "rdpFixedResolutionWidth": 1280, "rdpFixedResolutionHeight": 720"#,
            ),
            entry(
                "legacy-names",
                r#", "rdpDefaultResolutionWidth": 1024, "rdpDefaultResolutionHeight": 768"#,
            ),
            entry(
                "fixed-without-size",
                r#", "rdpResolutionMode": "Fixed", "rdpFixedResolutionWidth": 0"#,
            ),
            entry("one-side", r#", "rdpFixedResolutionWidth": 1280"#),
            entry("smart", r#", "rdpResolutionMode": "SmartSizing""#),
            entry(
                "multimon",
                r#", "rdpResolutionMode": "Multimon", "rdpFixedResolutionWidth": 1280,
                   "rdpFixedResolutionHeight": 720"#,
            ),
            entry(
                "auto",
                r#", "rdpResolutionMode": "Auto", "rdpDynamicResolution": false"#,
            ),
            entry(
                "too-large",
                r#", "rdpResolutionMode": "fixed", "rdpFixedResolutionWidth": 99999,
                   "rdpFixedResolutionHeight": 99999"#,
            ),
        ]
        .join(","),
    );
    let report = import(&json, None).expect("valid JSON");
    let taken: Vec<_> = report
        .rdp
        .iter()
        .map(|p| {
            let o = p.options;
            (
                p.id.as_str(),
                o.resolution,
                (o.fixed_width, o.fixed_height),
                o.scale_fixed,
                o.dynamic_resolution,
            )
        })
        .collect();
    let fit = Resolution::FitWindow;
    let fixed = Resolution::Fixed;
    assert_eq!(
        taken,
        [
            ("absent", fit, (1920, 1080), true, true),
            ("fixed", fixed, (1364, 768), false, true),
            ("sized-without-mode", fixed, (1280, 720), true, true),
            ("legacy-names", fixed, (1024, 768), true, true),
            // Without its size, the C# session follows the pane.
            ("fixed-without-size", fit, (1920, 1080), true, true),
            // Both sides are needed, as the C# migration asks.
            ("one-side", fit, (1920, 1080), true, true),
            ("smart", Resolution::SmartSizing, (1920, 1080), true, true),
            // Kept: a tab sizes them as it fits the window, the Windows client as they say.
            (
                "multimon",
                Resolution::MultiMonitor,
                (1280, 720),
                true,
                true
            ),
            ("auto", Resolution::Auto, (1920, 1080), true, false),
            ("too-large", fixed, (7680, 4320), true, true),
        ]
    );
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
            ("16", ColorDepth::Bpp16, AudioPlayback::Local),
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

#[test]
fn the_vault_entry_name_is_kept_for_the_credential_provider() {
    let json = servers(
        r#"{"id": "s", "remoteServer": "web.lab", "connectionType": "SSH", "vaultEntryName": "Servers/Web"},
           {"id": "r", "remoteServer": "dc.lab", "connectionType": "RDP", "vaultEntryName": "Windows/DC"},
           {"id": "v", "remoteServer": "kiosk.lab", "connectionType": "VNC", "vaultEntryName": "Kiosk"},
           {"id": "e", "remoteServer": "empty.lab", "connectionType": "SSH", "vaultEntryName": ""}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let ssh: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.vault_entry.as_deref()))
        .collect();
    assert_eq!(ssh, [("s", Some("Servers/Web")), ("e", None)]);
    assert_eq!(report.rdp[0].vault_entry.as_deref(), Some("Windows/DC"));
    assert_eq!(report.vnc[0].vault_entry.as_deref(), Some("Kiosk"));
}

#[test]
fn the_socks_proxy_port_is_imported_and_zero_opens_none() {
    let json = servers(
        r#"{"id": "s", "remoteServer": "web.lab", "connectionType": "SSH", "socksProxyPort": 1080},
           {"id": "r", "remoteServer": "dc.lab", "connectionType": "RDP", "socksProxyPort": 1081},
           {"id": "off", "remoteServer": "off.lab", "connectionType": "SSH", "socksProxyPort": 0},
           {"id": "absent", "remoteServer": "absent.lab", "connectionType": "SSH"},
           {"id": "big", "remoteServer": "big.lab", "connectionType": "SSH", "socksProxyPort": 70000},
           {"id": "neg", "remoteServer": "neg.lab", "connectionType": "RDP", "socksProxyPort": -1}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let ssh: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.forwards.socks_port))
        .collect();
    assert_eq!(ssh, [("s", Some(1080)), ("off", None), ("absent", None)]);
    assert_eq!(report.rdp.len(), 1);
    assert_eq!(report.rdp[0].forwards.socks_port, Some(1081));
    let skipped: Vec<_> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            ("big".to_owned(), SkipReason::InvalidPort(70000)),
            ("neg".to_owned(), SkipReason::InvalidPort(-1)),
        ]
    );
}

#[test]
fn the_remote_forward_is_imported_as_the_csharp_one_reads_it() {
    let json = servers(
        r#"{"id": "both", "remoteServer": "a.lab", "connectionType": "SSH", "remoteBindPort": 8080, "remoteLocalPort": 3000},
           {"id": "same", "remoteServer": "b.lab", "connectionType": "RDP", "remoteBindPort": 8081, "remoteLocalPort": 0},
           {"id": "off", "remoteServer": "c.lab", "connectionType": "SSH", "remoteBindPort": 0, "remoteLocalPort": 3000},
           {"id": "big", "remoteServer": "d.lab", "connectionType": "SSH", "remoteBindPort": 70000},
           {"id": "neg", "remoteServer": "e.lab", "connectionType": "SSH", "remoteBindPort": 8082, "remoteLocalPort": -1}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let ssh: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.forwards.remote()))
        .collect();
    assert_eq!(ssh, [("both", Some((8080, 3000))), ("off", None)]);
    assert_eq!(report.rdp[0].forwards.remote(), Some((8081, 8081)));
    let skipped: Vec<_> = report
        .skipped
        .into_iter()
        .map(|skipped| (skipped.id, skipped.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            ("big".to_owned(), SkipReason::InvalidPort(70000)),
            ("neg".to_owned(), SkipReason::InvalidPort(-1)),
        ]
    );
}

#[test]
fn post_connect_steps_are_imported_as_the_csharp_migration_reads_them_and_never_approved() {
    let json = servers(
        r#"{"id": "steps", "remoteServer": "a.lab", "connectionType": "SSH",
            "postConnectSteps": [
                {"input": "sudo -i", "delayMs": 500, "enabled": true, "onFailure": 1},
                null,
                {"input": "cd /srv", "delayMs": -5, "enabled": false, "onFailure": "Continue"},
                {"input": "ls", "onFailure": "Stop"}],
            "postConnectCommand": "ignored"},
           {"id": "legacy", "remoteServer": "b.lab", "connectionType": "SSH",
            "postConnectCommand": "sudo -i\n  \n cd /srv \n"},
           {"id": "none", "remoteServer": "c.lab", "connectionType": "SSH"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let by_id = |id: &str| {
        report
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == id)
            .expect(id)
            .post_connect
            .clone()
    };
    let steps = by_id("steps");
    assert_eq!(steps.approved, None);
    assert_eq!(
        steps.steps,
        [
            PostConnectStep {
                delay_ms: 500,
                on_failure: OnFailure::Stop,
                ..PostConnectStep::new("sudo -i")
            },
            PostConnectStep {
                delay_ms: 0,
                enabled: false,
                ..PostConnectStep::new("cd /srv")
            },
            PostConnectStep {
                on_failure: OnFailure::Stop,
                ..PostConnectStep::new("ls")
            },
        ]
    );
    assert_eq!(
        by_id("legacy").steps,
        [
            PostConnectStep::new("sudo -i"),
            PostConnectStep::new("cd /srv")
        ],
        "one step per line, as PostConnectMigration splits it"
    );
    assert_eq!(by_id("none"), PostConnect::default());
}

#[test]
fn a_step_linked_to_the_command_library_loses_its_link_and_the_report_counts_it() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};

    let json = servers(
        r#"{"id": "linked", "displayName": "Linked", "remoteServer": "a.lab",
            "connectionType": "SSH",
            "postConnectSteps": [
                {"input": "uptime", "commandLibraryId": "c1"},
                {"input": "whoami"},
                {"commandLibraryId": "c2", "enabled": false},
                {"commandLibraryId": "  "}]},
           {"id": "one", "remoteServer": "b.lab", "connectionType": "SSH",
            "sshX11Forwarding": true,
            "postConnectSteps": [{"input": "df -h", "commandLibraryId": "c3"}]},
           {"id": "plain", "remoteServer": "c.lab", "connectionType": "SSH",
            "postConnectSteps": [{"input": "ls"}]}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        report.dropped,
        [
            DroppedSettings {
                name: "Linked".to_owned(),
                // On or off, each link is lost; a blank one was no link.
                settings: vec![Dropped::CommandLibraryLinks(2)],
            },
            DroppedSettings {
                name: "b.lab".to_owned(),
                // X11 forwarding is carried for a shell in a tab too.
                settings: vec![Dropped::CommandLibraryLinks(1)],
            },
        ]
    );
    let linked = report
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == "linked")
        .expect("imported");
    let inputs: Vec<&str> = linked
        .post_connect
        .steps
        .iter()
        .map(|step| step.input.as_str())
        .collect();
    assert_eq!(
        inputs,
        ["uptime", "whoami", "", ""],
        "each step keeps its own text"
    );
}

#[test]
fn forwarding_the_agent_is_imported_off_unless_the_csharp_profile_turned_it_on() {
    let json = servers(
        r#"{"id": "on", "remoteServer": "a.lab", "connectionType": "SSH", "sshAgentForwarding": true, "sshCompression": true},
           {"id": "off", "remoteServer": "b.lab", "connectionType": "SSH", "sshAgentForwarding": false, "sshCompression": false},
           {"id": "unsaid", "remoteServer": "c.lab", "connectionType": "SSH"},
           {"id": "compressed", "remoteServer": "d.lab", "connectionType": "SSH", "sshCompression": true}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let options: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| {
            (
                profile.id.as_str(),
                profile.forward_agent,
                profile.compression,
            )
        })
        .collect();
    assert_eq!(
        options,
        [
            ("on", true, true),
            ("off", false, false),
            ("unsaid", false, false),
            ("compressed", false, true),
        ]
    );
}

#[test]
fn an_sftp_profile_is_imported_as_an_ssh_profile_that_opens_its_files() {
    let json = servers(
        r#"{"id": "files", "remoteServer": "a.lab", "connectionType": "SFTP", "sshUsername": "ops", "sshPort": 2222},
           {"id": "shell", "remoteServer": "b.lab", "connectionType": "SSH"},
           {"id": "lower", "remoteServer": "c.lab", "connectionType": "sftp"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let kinds: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.sftp))
        .collect();
    assert_eq!(kinds, [("files", true), ("shell", false)]);
    let files = &report.profiles[0];
    assert_eq!((files.username.as_deref(), files.port), (Some("ops"), 2222));
    // The C# compares the type as written, as for SSH.
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].id, "lower");
}

#[test]
fn an_ftp_profile_is_imported_as_the_csharp_one_reads_it() {
    let json = servers(
        r#"{"id": "files", "remoteServer": "ftp.lab", "connectionType": "FTP", "ftpPort": 2121,
            "ftpUsername": "ops", "ftpPassiveMode": false, "ftpUseSsl": true, "vaultEntryName": "Ftp/Ops"},
           {"id": "anon", "remoteServer": "pub.lab", "connectionType": "FTP", "ftpPort": 0},
           {"id": "big", "remoteServer": "big.lab", "connectionType": "FTP", "ftpPort": 70000}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert_eq!(report.ftp.len(), 2);
    let files = &report.ftp[0];
    assert_eq!(
        (
            files.port,
            files.username.as_deref(),
            files.passive,
            files.tls,
            files.vault_entry.as_deref()
        ),
        (2121, Some("ops"), false, true, Some("Ftp/Ops"))
    );
    let anon = &report.ftp[1];
    assert_eq!(
        (anon.port, anon.username.as_deref(), anon.passive, anon.tls),
        (21, None, true, false),
        "the C# defaults"
    );
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, SkipReason::InvalidPort(70000));
}

#[test]
fn a_citrix_profile_is_imported_without_its_workspace_cache_launch() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};
    let report = import(
        &servers(
            r#"{"id": "mail", "displayName": "Mail", "remoteServer": "", "connectionType": "Citrix",
                "citrixStoreFrontUrl": "https://store.lab/Citrix/Store", "citrixAppName": "Outlook",
                "citrixSeamlessMode": false, "citrixUseSso": false,
                "citrixLaunchCommandLine": "storebrowse.exe -launch secret-ticket"},
               {"id": "erp", "remoteServer": "", "connectionType": "citrix",
                "citrixAppName": " ERP ", "citrixIcaFilePath": "C:\\apps\\erp.ica",
                "citrixStoreFrontUrl": "   "}"#,
        ),
        None,
    )
    .expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(report.citrix.len(), 2);
    let mail = &report.citrix[0];
    assert_eq!(
        (
            mail.name.as_str(),
            mail.store_front_url.as_deref(),
            mail.app_name.as_deref(),
            mail.ica_file.as_deref(),
            mail.seamless,
            mail.sso
        ),
        (
            "Mail",
            Some("https://store.lab/Citrix/Store"),
            Some("Outlook"),
            None,
            false,
            false
        )
    );
    let erp = &report.citrix[1];
    assert_eq!(
        (
            erp.name.as_str(),
            erp.store_front_url.as_deref(),
            erp.ica_file.as_deref(),
            erp.seamless,
            erp.sso
        ),
        ("ERP", None, Some(r"C:\apps\erp.ica"), true, true),
        "named after its application, the C# defaults on"
    );
    assert_eq!(
        report.dropped,
        [DroppedSettings {
            name: "Mail".to_owned(),
            settings: vec![Dropped::CitrixCacheLaunch],
        }]
    );
    assert!(
        !format!("{:?}", report.citrix).contains("secret-ticket"),
        "the launch line is never kept"
    );
}

#[test]
fn what_a_profile_turned_on_that_has_no_equivalent_is_said_not_silently_dropped() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};
    let report = import(
        &servers(
            r#"
            {"id": "s", "displayName": "shell", "remoteServer": "s.lab", "connectionType": "SSH",
             "sshMode": "External", "sshX11Forwarding": true},
            {"id": "w", "displayName": "dc", "remoteServer": "dc.lab", "connectionType": "WINRM",
             "sshGatewayId": "g"},
            {"id": "r", "displayName": "desk", "remoteServer": "r.lab", "connectionType": "RDP",
             "rdpUseGlobalDefaults": false, "rdpRedirectPrinters": true,
             "rdpRedirectSmartCards": true, "rdpAntiIdle": true},
            {"id": "g1", "displayName": "global", "remoteServer": "g.lab", "connectionType": "RDP"},
            {"id": "plain", "displayName": "plain", "remoteServer": "p.lab", "connectionType": "SSH",
             "sshMode": "Embedded"}
            "#,
        ),
        Some(
            r#"{"rdpDefaultMode": "External", "rdpDefaultMultiMonitor": true,
                "sshGateways": [{"id": "g", "name": "Bastion", "host": "bastion.lab"}]}"#,
        ),
    )
    .expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // A WinRM session through a gateway is carried now, not dropped.
    assert_eq!(report.winrm[0].gateway, Some(ProfileId::new("g")));
    let dropped = |name: &str, settings: &[Dropped]| DroppedSettings {
        name: name.to_owned(),
        settings: settings.to_vec(),
    };
    assert_eq!(
        report.dropped,
        [
            // A shell in PuTTY with X11 forwarding is carried now, not dropped.
            // Anti-idle is carried over, not dropped.
            dropped("desk", &[Dropped::RdpPrinters, Dropped::RdpSmartCards]),
            // On the global defaults: the settings' choices are the ones dropped.
            dropped(
                "global",
                &[Dropped::ExternalClient, Dropped::RdpMultiMonitor]
            ),
        ]
    );
}

#[test]
fn the_trusted_ssh_servers_of_settings_are_read_with_their_keys_when_kept() {
    use heimdall_core::import::csharp::{TrustedHostKey, TrustedHostKeySource};
    let pinned = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let other = "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
    let settings_json = format!(
        r#"{{
            "trustedHostKeysV2": {{
                "web.lab:22": {{"fingerprint": "{pinned}", "publicKeyBase64": "AAAAC3Nz"}},
                "[fe80::1]:2222": {{"fingerprint": "{pinned}"}},
                "noport": {{"fingerprint": "{pinned}"}},
                "empty.lab:22": {{"fingerprint": ""}}
            }},
            "trustedHostKeys": {{
                "web.lab:22": "{other}",
                "old.lab:2200": "{other}"
            }},
            "sshGateways": [
                {{"id": "g", "name": "jump", "host": "jump.lab", "hostKeyFingerprint": "{other}"}},
                {{"id": "h", "name": "web again", "host": "WEB.lab", "port": 22,
                  "hostKeyFingerprint": "{other}"}}
            ]
        }}"#
    );
    let report = import(&servers(""), Some(&settings_json)).expect("valid JSON");
    let entry = |host: &str, port, fingerprint: &str, key: Option<&str>| TrustedHostKey {
        host: host.to_owned(),
        port,
        fingerprint: fingerprint.to_owned(),
        key: key.map(str::to_owned),
        source: TrustedHostKeySource::Unknown,
        first_seen: None,
        last_seen: None,
    };
    assert_eq!(
        report.host_keys,
        [
            entry("fe80::1", 2222, pinned, None),
            entry("web.lab", DEFAULT_SSH_PORT, pinned, Some("AAAAC3Nz")),
            // The first store, for a server the second does not name.
            entry("old.lab", 2200, other, None),
            // A gateway's own fingerprint, its port the SSH default.
            entry("jump.lab", DEFAULT_SSH_PORT, other, None),
        ]
    );
}

#[test]
fn a_trusted_server_s_dates_and_source_are_carried_as_the_csharp_wrote_them() {
    use std::time::{Duration, UNIX_EPOCH};

    use heimdall_core::import::csharp::TrustedHostKeySource;
    let print = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let settings_json = format!(
        r#"{{
            "trustedHostKeysV2": {{
                "user.lab:22": {{"fingerprint": "{print}", "source": "UserConfirmed",
                    "firstSeen": "2026-03-15T13:00:30.1234567+01:00",
                    "lastSeen": "2026-09-27T19:15:03+00:00"}},
                "imported.lab:22": {{"fingerprint": "{print}", "source": 2,
                    "firstSeen": "0001-01-01T00:00:00+00:00", "lastSeen": "not a date"}},
                "factory.lab:22": {{"fingerprint": "{print}", "source": "Factory",
                    "firstSeen": 12}},
                "odd.lab:22": {{"fingerprint": "{print}", "source": {{"weird": true}}}}
            }}
        }}"#
    );
    let report = import(&servers(""), Some(&settings_json)).expect("valid JSON");
    let of = |host: &str| {
        report
            .host_keys
            .iter()
            .find(|entry| entry.host == host)
            .expect(host)
    };
    let user = of("user.lab");
    assert_eq!(user.source, TrustedHostKeySource::UserConfirmed);
    assert_eq!(
        user.first_seen,
        Some(UNIX_EPOCH + Duration::from_secs(1_773_576_030)),
        "2026-03-15 12:00:30 UTC"
    );
    assert_eq!(
        user.last_seen,
        Some(UNIX_EPOCH + Duration::from_secs(1_790_536_503))
    );
    let imported = of("imported.lab");
    assert_eq!(imported.source, TrustedHostKeySource::ImportedKnownHosts);
    assert_eq!(
        (imported.first_seen, imported.last_seen),
        (None, None),
        "MinValue and a text that is no date are unknown"
    );
    assert_eq!(
        of("factory.lab").source,
        TrustedHostKeySource::Unknown,
        "no factory keys here"
    );
    assert_eq!(of("factory.lab").first_seen, None);
    assert_eq!(of("odd.lab").source, TrustedHostKeySource::Unknown);
}

#[test]
fn the_favorites_are_imported_with_their_profiles() {
    let json = servers(
        r#"{"id": "fav", "displayName": "Web", "remoteServer": "web.lab", "connectionType": "SSH", "isFavorite": true},
           {"id": "plain", "displayName": "Db", "remoteServer": "db.lab", "connectionType": "SSH"},
           {"id": "skipped", "displayName": "", "remoteServer": "", "connectionType": "SSH", "isFavorite": true}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert_eq!(
        report.favorites,
        [ProfileId::new("fav")],
        "a profile left out is not one"
    );
}

#[test]
fn a_profile_s_session_logging_is_imported_as_the_csharp_override_says() {
    let json = servers(
        r#"{"id": "on", "remoteServer": "a.lab", "connectionType": "SSH", "sessionLoggingOverride": true},
           {"id": "off", "remoteServer": "b.lab", "connectionType": "SSH", "sessionLoggingOverride": false},
           {"id": "inherit", "remoteServer": "c.lab", "connectionType": "SSH", "sessionLoggingOverride": null},
           {"id": "unsaid", "remoteServer": "d.lab", "connectionType": "SSH"},
           {"id": "switch", "remoteServer": "e.lab", "connectionType": "Telnet", "sessionLoggingOverride": false}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    let ssh: Vec<_> = report
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.session_logging))
        .collect();
    assert_eq!(
        ssh,
        [
            ("on", Some(true)),
            ("off", Some(false)),
            ("inherit", None),
            ("unsaid", None),
        ]
    );
    assert_eq!(report.telnet[0].session_logging, Some(false));
}

#[test]
fn the_metadata_section_is_imported_what_does_not_read_left_out() {
    use heimdall_core::metadata::{Environment, MacAddress};

    let report = import(
        &servers(
            r#"{"id": "a", "remoteServer": "a.lab", "connectionType": "SSH", "environment": "Production", "tags": "web, prod", "macAddress": "AA:BB:CC:DD:EE:FF"},
               {"id": "b", "remoteServer": "b.lab", "connectionType": "RDP", "environment": "None", "macAddress": "bad"},
               {"id": "c", "remoteServer": "c.lab", "connectionType": "SSH"}"#,
        ),
        None,
    )
    .expect("valid JSON");
    assert_eq!(report.metadata.len(), 1, "{:?}", report.metadata);
    let (id, metadata) = &report.metadata[0];
    assert_eq!(id.as_str(), "a");
    assert_eq!(metadata.environment, Some(Environment::Production));
    assert_eq!(metadata.tags, "web, prod");
    assert_eq!(
        metadata.mac_address,
        Some(MacAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]))
    );
}

#[test]
fn the_folders_colours_of_settings_json_are_imported_those_of_the_palette_only() {
    use heimdall_core::folder::FolderColor;

    let report = import(
        &servers(
            r#"{"id": "a", "remoteServer": "a.lab", "connectionType": "SSH", "group": "Lab"}"#,
        ),
        Some(&settings(
            r##""Lab": {"color": "#3b82f6"}, "Lab/Linux": {"color": "#F97316"},
               "Odd": {"color": "#123456"}, "Plain": {"sshUsername": "ops"}"##,
        )),
    )
    .expect("valid JSON");
    assert_eq!(
        report.folder_colors,
        [
            ("Lab".to_owned(), FolderColor::Blue),
            ("Lab/Linux".to_owned(), FolderColor::Orange),
        ]
    );
}

#[test]
fn an_rdp_profile_keeps_what_the_built_in_client_does_not_do_and_where_it_came_from() {
    use heimdall_core::import::csharp::Dropped;
    use heimdall_core::metadata::ProfileOrigin;
    use heimdall_core::profile::{Aspect, RdpExtras, Resolution};

    let json = servers(
        r#"{"id": "far", "displayName": "Far", "remoteServer": "far.lab", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false, "rdpMode": "External", "rdpGateway": " rdg.lab ",
            "rdpRedirectPrinters": true, "rdpRedirectSmartCards": true, "rdpAudioCapture": true,
            "rdpMultiMonitor": true, "rdpSelectedMonitorIndices": [0, 2, -1],
            "rdpStrictServerAuthentication": true, "rdpDisableUdp": true,
            "rdpBitmapCaching": false, "rdpFullScreen": true, "rdpHardwareAcceleration": true,
            "rdpResolutionMode": "Auto", "rdpAspectRatio": "4:3",
            "origin": 6, "sortOrder": 4, "tunnelsPanelExpanded": false},
           {"id": "near", "remoteServer": "near.lab", "connectionType": "RDP",
            "rdpResolutionMode": "Multimon", "rdpAspectRatio": "Preserve", "origin": "Manual"}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let (far, near) = (&report.rdp[0], &report.rdp[1]);
    assert_eq!(
        far.extras,
        RdpExtras {
            external: true,
            rd_gateway: Some("rdg.lab".to_owned()),
            redirect_printers: true,
            redirect_smart_cards: true,
            microphone: true,
            multi_monitor: true,
            monitors: vec![0, 2],
            strict_server_authentication: true,
            disable_udp: true,
            bitmap_caching: false,
            hardware_acceleration: true,
            full_screen: true,
            ..RdpExtras::default()
        }
    );
    assert_eq!(
        (far.options.resolution, far.options.aspect),
        (Resolution::Auto, Aspect::Standard)
    );
    assert_eq!(
        (near.options.resolution, near.options.aspect, &near.extras),
        (
            Resolution::MultiMonitor,
            Aspect::Stretch,
            &RdpExtras::default()
        ),
        "Preserve fills the tab, as the C# draws it"
    );
    let far_metadata = report
        .metadata
        .iter()
        .find(|(id, _)| id.as_str() == "far")
        .map(|(_, metadata)| metadata)
        .expect("far says something");
    assert_eq!(
        (
            far_metadata.origin,
            far_metadata.sort_order,
            far_metadata.tunnels_expanded
        ),
        (Some(ProfileOrigin::RdcMan), Some(4), Some(false))
    );
    assert!(
        report.metadata.iter().all(|(id, _)| id.as_str() != "near"),
        "made by hand and in no particular place: nothing to keep"
    );
    assert_eq!(
        report.dropped[0].settings,
        [
            Dropped::ExternalClient,
            Dropped::RdGateway,
            Dropped::RdpPrinters,
            Dropped::RdpSmartCards,
            Dropped::RdpMicrophone,
            Dropped::RdpMultiMonitor,
        ],
        "said, as the built-in client does not use them"
    );
}

#[test]
fn the_ssh_mode_and_x11_forwarding_of_a_shell_are_carried_and_dropped_where_nothing_does_them() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};
    use heimdall_core::profile::SshMode;

    let report = import(
        &servers(
            r#"
            {"id": "putty", "displayName": "putty", "remoteServer": "a.lab",
             "connectionType": "SSH", "sshMode": " external ", "sshX11Forwarding": true},
            {"id": "tab", "displayName": "tab", "remoteServer": "b.lab", "connectionType": "SSH",
             "sshMode": "Embedded", "sshX11Forwarding": true},
            {"id": "plain", "displayName": "plain", "remoteServer": "c.lab",
             "connectionType": "SSH"},
            {"id": "files", "displayName": "files", "remoteServer": "d.lab",
             "connectionType": "SFTP", "sshMode": "External", "sshX11Forwarding": true}
            "#,
        ),
        None,
    )
    .expect("valid JSON");
    let profile = |id: &str| {
        report
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == id)
            .expect("imported")
    };
    assert_eq!(profile("putty").ssh_mode, SshMode::External);
    assert!(profile("putty").x11_forwarding);
    assert_eq!(profile("tab").ssh_mode, SshMode::Embedded);
    assert!(
        profile("tab").x11_forwarding,
        "the built-in terminal forwards it"
    );
    assert_eq!(
        profile("plain").ssh_mode,
        SshMode::Embedded,
        "the C# default"
    );
    assert!(!profile("plain").x11_forwarding);
    assert_eq!(profile("files").ssh_mode, SshMode::Embedded);
    assert!(!profile("files").x11_forwarding);
    let dropped = |name: &str, settings: &[Dropped]| DroppedSettings {
        name: name.to_owned(),
        settings: settings.to_vec(),
    };
    assert_eq!(
        report.dropped,
        [
            // The built-in terminal forwards X11 too: only the files tab drops it.
            dropped("files", &[Dropped::ExternalClient, Dropped::X11Forwarding]),
        ]
    );
}

#[test]
fn an_rdp_profile_s_own_wait_after_connecting_is_kept_and_one_out_of_range_dropped_with_a_note() {
    use heimdall_core::import::csharp::{Dropped, DroppedSettings};

    let json = servers(
        r#"{"id": "own", "displayName": "Own", "remoteServer": "own.lab", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false, "rdpResizeEnableDelayMs": 5000},
           {"id": "off", "displayName": "Off", "remoteServer": "off.lab", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false, "rdpResizeEnableDelayMs": 0},
           {"id": "global", "displayName": "Global", "remoteServer": "g.lab", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false},
           {"id": "short", "displayName": "Short", "remoteServer": "s.lab", "connectionType": "RDP",
            "rdpUseGlobalDefaults": false, "rdpResizeEnableDelayMs": 500},
           {"id": "negative", "displayName": "Negative", "remoteServer": "n.lab",
            "connectionType": "RDP", "rdpUseGlobalDefaults": false, "rdpResizeEnableDelayMs": -1}"#,
    );
    let report = import(&json, None).expect("valid JSON");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let delays: Vec<Option<u32>> = report
        .rdp
        .iter()
        .map(|profile| profile.options.resize_enable_delay_ms)
        .collect();
    assert_eq!(
        delays,
        [Some(5_000), Some(0), None, None, None],
        "out of the range: the global setting, never brought within it"
    );
    assert_eq!(
        report.dropped,
        [
            DroppedSettings {
                name: "Short".to_owned(),
                settings: vec![Dropped::RdpResizeDelayOutOfRange(500)],
            },
            DroppedSettings {
                name: "Negative".to_owned(),
                settings: vec![Dropped::RdpResizeDelayOutOfRange(-1)],
            },
        ]
    );
}

/// The local tunnel port imported for `local_port` written as a JSON number, on a profile of
/// `connection_type`, with `settings_json`; `Err` with the reason when the profile is
/// refused.
fn imported_local_port(
    connection_type: &str,
    local_port: &str,
    settings_json: Option<&str>,
) -> Result<Option<u16>, SkipReason> {
    let json = servers(&format!(
        r#"{{"id": "a", "remoteServer": "h", "connectionType": "{connection_type}",
            "sshGatewayId": "gw", "localPort": {local_port}}}"#
    ));
    let report = import(&json, settings_json).expect("valid JSON");
    if let Some(skipped) = report.skipped.into_iter().next() {
        return Err(skipped.reason);
    }
    Ok(report
        .profiles
        .first()
        .map(|profile| profile.local_tunnel_port)
        .or_else(|| report.rdp.first().map(|profile| profile.local_tunnel_port))
        .or_else(|| {
            report
                .winrm
                .first()
                .map(|profile| profile.local_tunnel_port)
        })
        .expect("one profile"))
}

#[test]
fn the_local_tunnel_port_is_read_as_the_csharp_reads_it() {
    // Zero or less, or the type's suggested port: the automatic choice. Above 65535: the
    // profile refused, as the C# `ImportedProfileValidator` refuses it.
    for (kind, value, expected) in [
        ("RDP", "0", Ok(None)),
        ("RDP", "-1", Ok(None)),
        ("RDP", "33890", Ok(None)),
        ("RDP", "2222", Ok(Some(2222))),
        ("RDP", "40000", Ok(Some(40000))),
        (
            "RDP",
            "70000",
            Err(SkipReason::InvalidLocalTunnelPort(70000)),
        ),
        ("SSH", "2222", Ok(None)),
        ("SSH", "33890", Ok(Some(33890))),
        ("SSH", "40000", Ok(Some(40000))),
        ("SFTP", "2222", Ok(None)),
        ("SSH", "-1", Ok(None)),
        (
            "SSH",
            "70000",
            Err(SkipReason::InvalidLocalTunnelPort(70000)),
        ),
        ("WINRM", "59850", Ok(None)),
        ("WINRM", "0", Ok(None)),
        ("WINRM", "40000", Ok(Some(40000))),
        (
            "WINRM",
            "70000",
            Err(SkipReason::InvalidLocalTunnelPort(70000)),
        ),
    ] {
        assert_eq!(
            imported_local_port(kind, value, None),
            expected,
            "{kind} {value}"
        );
    }
}

#[test]
fn an_absent_local_tunnel_port_is_the_automatic_choice() {
    for kind in ["RDP", "SSH", "WINRM"] {
        let json = servers(&format!(
            r#"{{"id": "a", "remoteServer": "h", "connectionType": "{kind}"}}"#
        ));
        let report = import(&json, None).expect("valid JSON");
        assert!(report.skipped.is_empty(), "{kind}");
        assert!(
            report
                .profiles
                .iter()
                .all(|p| p.local_tunnel_port.is_none())
                && report.rdp.iter().all(|p| p.local_tunnel_port.is_none())
                && report.winrm.iter().all(|p| p.local_tunnel_port.is_none()),
            "{kind}"
        );
    }
}

#[test]
fn the_suggested_tunnel_ports_of_the_csharp_settings_are_the_automatic_choice() {
    let settings = r#"{"defaultRdpTunnelPort": 40000, "defaultSshTunnelPort": 40022}"#;
    assert_eq!(
        imported_local_port("RDP", "40000", Some(settings)),
        Ok(None)
    );
    assert_eq!(
        imported_local_port("RDP", "33890", Some(settings)),
        Ok(Some(33890)),
        "no longer the suggested one: chosen"
    );
    assert_eq!(
        imported_local_port("SSH", "40022", Some(settings)),
        Ok(None)
    );
    assert_eq!(
        imported_local_port("SSH", "2222", Some(settings)),
        Ok(Some(2222))
    );
    // A constant in the C#, whatever the settings.
    assert_eq!(
        imported_local_port("WINRM", "59850", Some(settings)),
        Ok(None)
    );
    // Out of range in the settings: the C# default, as its `SettingRange` brings it back.
    let wrong = r#"{"defaultRdpTunnelPort": 0, "defaultSshTunnelPort": 70000}"#;
    assert_eq!(imported_local_port("RDP", "33890", Some(wrong)), Ok(None));
    assert_eq!(imported_local_port("SSH", "2222", Some(wrong)), Ok(None));
}
