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

//! Export in the C# Heimdall's session document: what is written comes back the same through
//! the import, and the document has the C# shape.

use std::path::PathBuf;

use heimdall_core::export;
use heimdall_core::import::csharp::import;
use heimdall_core::post_connect::{OnFailure, PostConnect, PostConnectStep};
use heimdall_core::profile::{
    AudioPlayback, ColorDepth, Forwards, FtpProfile, LocalArguments, LocalCommand, LocalProfile,
    ProfileId, RdpDefaults, RdpOptions, RdpProfile, Resolution, SshGateway, SshProfile,
    TelnetProfile, VncProfile, WinRmProfile,
};
use heimdall_core::store::ProfileStore;

fn gateways() -> Vec<SshGateway> {
    vec![
        SshGateway {
            id: ProfileId::new("edge"),
            name: "Edge".to_owned(),
            host: "edge.lab".to_owned(),
            port: 2222,
            username: Some("jump".to_owned()),
            key_path: Some(PathBuf::from(r"C:\keys\edge")),
            parent: None,
        },
        SshGateway {
            id: ProfileId::new("inner"),
            name: "Inner".to_owned(),
            host: "inner.lab".to_owned(),
            port: 22,
            username: None,
            key_path: None,
            parent: Some(ProfileId::new("edge")),
        },
    ]
}

fn ssh() -> Vec<SshProfile> {
    let base = SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: Some("Prod/Web".to_owned()),
        host: "web.lab".to_owned(),
        port: 2200,
        username: Some("ops".to_owned()),
        key_path: Some(PathBuf::from(r"C:\keys\web")),
        gateway: Some(ProfileId::new("inner")),
        vault_entry: Some("Web/Ops".to_owned()),
        forwards: Forwards {
            socks_port: Some(1080),
            remote_bind_port: Some(9000),
            remote_local_port: Some(8000),
        },
        post_connect: PostConnect {
            steps: vec![
                PostConnectStep::new("sudo -i"),
                PostConnectStep {
                    input: "cd /srv".to_owned(),
                    delay_ms: 50,
                    enabled: false,
                    on_failure: OnFailure::Stop,
                },
            ],
            approved: None,
        },
        forward_agent: true,
        compression: true,
        sftp: false,
        legacy_algorithms: false,
    };
    let files = SshProfile {
        id: ProfileId::new("files"),
        name: "Files".to_owned(),
        group: None,
        host: "files.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: true,
        legacy_algorithms: false,
    };
    vec![base, files]
}

fn rdp() -> Vec<RdpProfile> {
    vec![
        RdpProfile {
            id: ProfileId::new("dc"),
            name: "DC".to_owned(),
            group: Some("Windows".to_owned()),
            host: "dc.lab".to_owned(),
            port: 3390,
            username: Some("admin".to_owned()),
            domain: Some("LAB".to_owned()),
            allow_tls_only: true,
            gateway: Some(ProfileId::new("edge")),
            redirect_clipboard: false,
            redirect_drives: true,
            options: RdpOptions {
                color_depth: ColorDepth::Bpp16,
                audio: AudioPlayback::OnServer,
                admin_session: true,
                resolution: Resolution::Fixed,
                fixed_width: 1280,
                fixed_height: 720,
                scale_fixed: false,
                dynamic_resolution: false,
                // Wallpaper off, font smoothing on.
                performance_flags: 0x81,
            },
            vault_entry: Some("Win/DC".to_owned()),
            forwards: Forwards::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: true,
        },
        RdpProfile {
            id: ProfileId::new("desk"),
            name: "Desk".to_owned(),
            group: None,
            host: "desk.lab".to_owned(),
            port: 3389,
            username: None,
            domain: None,
            allow_tls_only: false,
            gateway: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options: RdpOptions {
                audio: AudioPlayback::Local,
                resolution: Resolution::SmartSizing,
                ..RdpOptions::default()
            },
            vault_entry: None,
            forwards: Forwards::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
        },
    ]
}

fn local(arguments: LocalArguments) -> LocalProfile {
    LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: Some("Tools".to_owned()),
        command: LocalCommand {
            program: Some(r"C:\Tools\tool.exe".to_owned()),
            arguments,
            working_directory: Some(PathBuf::from(r"C:\Work")),
        },
        approved: None,
    }
}

fn store(dir: &std::path::Path) -> ProfileStore {
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    store.merge_gateways(gateways());
    store.merge(ssh());
    store.merge_rdp(rdp());
    store.merge_telnet([TelnetProfile {
        id: ProfileId::new("switch"),
        name: "Switch".to_owned(),
        group: None,
        host: "switch.lab".to_owned(),
        port: 2323,
    }]);
    store.merge_vnc([VncProfile {
        id: ProfileId::new("screen"),
        name: "Screen".to_owned(),
        group: None,
        host: "screen.lab".to_owned(),
        port: 5901,
        view_only: true,
        allow_no_password: false,
        vault_entry: None,
    }]);
    store.merge_ftp([FtpProfile {
        id: ProfileId::new("ftp"),
        name: "FTP".to_owned(),
        group: None,
        host: "ftp.lab".to_owned(),
        port: 2121,
        username: Some("ops".to_owned()),
        passive: false,
        tls: true,
        vault_entry: Some("Ftp/Ops".to_owned()),
    }]);
    store.merge_local([local(LocalArguments::WindowsLine(
        r#"/c "echo hi""#.to_owned(),
    ))]);
    store.merge_winrm([
        WinRmProfile {
            id: ProfileId::new("ps"),
            name: "PS".to_owned(),
            group: None,
            host: "ps.lab".to_owned(),
            port: 5986,
            use_ssl: true,
            skip_certificate_check: true,
            username: Some(r"LAB\admin".to_owned()),
            gateway: None,
        },
        WinRmProfile {
            id: ProfileId::new("ps-me"),
            name: "PS as me".to_owned(),
            group: None,
            host: "ps2.lab".to_owned(),
            port: 5985,
            use_ssl: false,
            skip_certificate_check: false,
            username: None,
            // Through a gateway, HTTP only.
            gateway: Some(ProfileId::new("edge")),
        },
    ]);
    store
}

#[test]
fn every_profile_comes_back_the_same_through_the_import() {
    let dir = tempfile::tempdir().expect("dir");
    let store = store(dir.path());
    let report = import(
        &export::csharp(&store, &line, &RdpDefaults::default()),
        None,
    )
    .expect("the importer reads the export");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        report.gateways,
        store.gateways(),
        "from the document's gateways"
    );
    assert_eq!(report.profiles, store.ssh_profiles());
    assert_eq!(report.rdp, store.rdp_profiles());
    assert_eq!(report.telnet, store.telnet_profiles());
    assert_eq!(
        report.vnc,
        store.vnc_profiles(),
        "a server asking a password still must"
    );
    assert_eq!(report.ftp, store.ftp_profiles());
    assert_eq!(report.local, store.local_profiles());
    assert_eq!(report.winrm, store.winrm_profiles());
    assert_eq!(export::session_count(&store), 10);
}

/// A Windows line as written; a list, which this test does not quote, marked.
fn line(arguments: &LocalArguments) -> String {
    match arguments {
        LocalArguments::WindowsLine(line) => line.clone(),
        LocalArguments::List(words) => format!("quoted:{}", words.join("|")),
    }
}

#[test]
fn listed_arguments_are_written_as_the_given_windows_line() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_local([local(LocalArguments::List(vec![
        "/c".to_owned(),
        "echo hi".to_owned(),
    ]))]);
    let report = import(
        &export::csharp(&store, &line, &RdpDefaults::default()),
        None,
    )
    .expect("reads");
    assert_eq!(
        report.local[0].command.arguments,
        LocalArguments::WindowsLine("quoted:/c|echo hi".to_owned())
    );
}

#[test]
fn the_document_has_the_csharp_shape_and_no_secret() {
    let dir = tempfile::tempdir().expect("dir");
    let text = export::csharp(&store(dir.path()), &line, &RdpDefaults::default());
    let document: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(document["schemaVersion"], 2);
    let servers = document["servers"].as_array().expect("servers");
    let by_id = |id: &str| {
        servers
            .iter()
            .find(|server| server["id"] == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    let kinds: Vec<(&str, &str)> = [
        ("web", "SSH"),
        ("files", "SFTP"),
        ("dc", "RDP"),
        ("switch", "Telnet"),
        ("screen", "VNC"),
        ("ftp", "FTP"),
        ("tool", "Local"),
        ("ps", "WINRM"),
    ]
    .into();
    for (id, kind) in kinds {
        assert_eq!(by_id(id)["connectionType"], kind, "{id}");
    }
    let dc = by_id("dc");
    assert_eq!(dc["rdpUseGlobalDefaults"], false, "every choice its own");
    assert_eq!(dc["remotePort"], 3390);
    assert_eq!(dc["rdpAudioMode"], 2);
    assert_eq!(dc["rdpAntiIdle"], true);
    assert_eq!(dc["rdpPerformanceFlags"], 0x81);
    assert_eq!(dc["rdpColorDepth"], 16);
    assert_eq!(dc["useDirectConnection"], false);
    assert_eq!(by_id("desk")["useDirectConnection"], true);
    assert_eq!(by_id("ps")["winRmIdentityMode"], "Credential");
    assert_eq!(by_id("ps-me")["winRmIdentityMode"], "CurrentUser");
    assert_eq!(by_id("web")["postConnectSteps"][1]["onFailure"], 1);
    assert!(by_id("web").get("postConnectCommand").is_none());
    assert_eq!(document["gateways"][1]["parentGatewayId"], "edge");
    for secret in ["assword", "Encrypted", "passphrase"] {
        let names = servers
            .iter()
            .chain(document["gateways"].as_array().expect("gateways"))
            .flat_map(|entry| entry.as_object().expect("object").keys())
            .filter(|key| key.contains(secret) && *key != "vncAllowNoPassword")
            .collect::<Vec<_>>();
        assert!(names.is_empty(), "{names:?}");
    }
}

#[test]
fn a_csharp_export_of_the_first_format_is_read_too() {
    let report = import(
        r#"[{"id": "web", "remoteServer": "web.lab", "connectionType": "SSH"}]"#,
        None,
    )
    .expect("a bare array of servers");
    assert_eq!(report.profiles.len(), 1);
}

#[test]
fn a_gateway_in_both_the_document_and_the_settings_is_the_documents() {
    let report = import(
        r#"{"servers": [], "gateways": [{"id": "edge", "host": "new.lab"}]}"#,
        Some(r#"{"sshGateways": [{"id": "edge", "host": "old.lab"}, {"id": "other", "host": "o.lab"}]}"#),
    )
    .expect("reads");
    let hosts: Vec<&str> = report.gateways.iter().map(|g| g.host.as_str()).collect();
    assert_eq!(hosts, ["new.lab", "o.lab"]);
}
