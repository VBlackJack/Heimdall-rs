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
use heimdall_core::profile::{DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, SshProfile};

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
fn an_rdp_profile_through_a_gateway_is_left_out_unless_direct() {
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
            ("tunnel".to_owned(), SkipReason::NeedsJumpHost),
            ("rdg".to_owned(), SkipReason::NeedsRdGateway),
            ("port".to_owned(), SkipReason::InvalidPort(0)),
        ]
    );
}

#[test]
fn every_skip_reason_is_reported() {
    let json = servers(
        r#"{"id": "vnc", "remoteServer": "h", "connectionType": "VNC"},
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
            ("vnc".to_owned(), SkipReason::NotSsh("VNC".to_owned())),
            ("gw".to_owned(), SkipReason::NeedsJumpHost),
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
fn a_group_default_gateway_makes_the_profile_need_a_jump_host() {
    let json =
        servers(r#"{"id": "a", "remoteServer": "h", "connectionType": "SSH", "group": "G"}"#);
    let defaults = settings(r#""G": {"sshGatewayId": "gw-1"}"#);
    let report = import(&json, Some(&defaults)).expect("valid JSON");
    assert_eq!(report.skipped[0].reason, SkipReason::NeedsJumpHost);
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
           {"id": "tls", "remoteServer": "h", "connectionType": "RDP", "rdpNla": false},
           {"id": "explicit", "remoteServer": "h", "connectionType": "RDP", "rdpNla": true}"#,
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
