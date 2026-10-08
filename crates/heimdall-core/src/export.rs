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

//! Export of the profiles in the C# Heimdall's session file, so both apps exchange them.
//!
//! The document is the one the C# "Export Sessions" writes: `schemaVersion` 2, the
//! `servers` and the SSH `gateways`, camelCase, no secret. Every choice is written out, RDP
//! included (`rdpUseGlobalDefaults` off), so the file means the same whatever the settings of
//! the app that reads it. What the C# has no key for is written under a key of its own,
//! which the C# ignores and [`crate::import::csharp::import`] reads back.

use serde::Serialize;

use crate::metadata::ProfileOrigin;
use crate::post_connect::{OnFailure, PostConnectStep};
use crate::profile::{
    Aspect, AudioPlayback, CitrixProfile, Forwards, FtpProfile, LocalArguments, LocalProfile,
    RdpDefaults, RdpProfile, Resolution, SshGateway, SshMode, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile,
};
use crate::store::ProfileStore;

/// Version of the C# session document written.
pub const SCHEMA_VERSION: u32 = 2;

/// Name the C# gives the exported file.
pub const EXPORT_FILE_NAME: &str = "servers.json";

/// `connectionType` of each protocol, as the C# `ConnectionTypeCatalog` spells it.
mod connection_type {
    pub const SSH: &str = "SSH";
    pub const SFTP: &str = "SFTP";
    pub const RDP: &str = "RDP";
    pub const TELNET: &str = "Telnet";
    pub const VNC: &str = "VNC";
    pub const FTP: &str = "FTP";
    pub const LOCAL: &str = "Local";
    pub const WINRM: &str = "WINRM";
    pub const CITRIX: &str = "Citrix";
}

/// The C# `RdpAudioMode` values.
const AUDIO_OFF: i64 = 0;
const AUDIO_LOCAL: i64 = 1;
const AUDIO_ON_SERVER: i64 = 2;

/// The C# `PostConnectFailurePolicy` values.
const CONTINUE_POLICY: i64 = 0;
const STOP_POLICY: i64 = 1;

/// The C# `WinRmIdentityMode` names.
const WINRM_CURRENT_USER: &str = "CurrentUser";
const WINRM_CREDENTIAL: &str = "Credential";

/// The C# `ElevationMode` "External window", by value, as the C# writes it: a local shell run
/// as administrator in a window of its own.
const RUNAS_ELEVATION: i64 = 3;

/// The whole document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    schema_version: u32,
    servers: Vec<Entry>,
    gateways: Vec<Gateway>,
}

/// One profile, with the keys its protocol has; the others are left out.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the flags of the C# server JSON, one field per key"
)]
struct Entry {
    id: String,
    display_name: String,
    remote_server: String,
    connection_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vault_entry_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_gateway_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_logging_override: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mac_address: Option<String>,
    /// The C# `ProfileOrigin`, by number; absent is `Manual`.
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort_order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tunnels_panel_expanded: Option<bool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ssh_agent_forwarding: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ssh_compression: bool,
    /// The C# `SshMode`, written only for a shell opened in `PuTTY`: absent is embedded.
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ssh_x11_forwarding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    socks_proxy_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_bind_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_local_port: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    post_connect_steps: Vec<Step>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    rdp: Option<RdpKeys>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telnet_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vnc_port: Option<u16>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    vnc_view_only: bool,
    /// Heimdall-rs's own key: the C# derives it from its stored password, which is never
    /// exported.
    #[serde(skip_serializing_if = "Option::is_none")]
    vnc_allow_no_password: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vnc_require_tls: Option<bool>,
    /// Heimdall-rs's own key: the C# has no VNC user name.
    #[serde(skip_serializing_if = "Option::is_none")]
    vnc_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ftp_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ftp_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ftp_passive_mode: Option<bool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ftp_use_ssl: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    citrix_store_front_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    citrix_app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    citrix_ica_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    citrix_seamless_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    citrix_use_sso: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_shell_executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_shell_arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_shell_working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elevation_mode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    win_rm_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    win_rm_username: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    win_rm_use_ssl: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    win_rm_skip_certificate_check: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    win_rm_identity_mode: Option<&'static str>,
}

/// The RDP keys, every choice written out.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the flags of the C# server JSON, one field per key"
)]
struct RdpKeys {
    remote_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    rdp_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rdp_domain: Option<String>,
    use_direct_connection: bool,
    rdp_use_global_defaults: bool,
    rdp_nla: bool,
    rdp_redirect_clipboard: bool,
    rdp_redirect_drives: bool,
    rdp_color_depth: u32,
    rdp_audio_mode: i64,
    rdp_admin_mode: bool,
    rdp_resolution_mode: &'static str,
    #[serde(rename = "rdpFixedResolutionWidth")]
    rdp_fixed_width: u16,
    #[serde(rename = "rdpFixedResolutionHeight")]
    rdp_fixed_height: u16,
    rdp_initial_smart_sizing: bool,
    rdp_dynamic_resolution: bool,
    rdp_anti_idle: bool,
    rdp_auto_reconnect: bool,
    rdp_performance_flags: u32,
    rdp_aspect_ratio: &'static str,
    rdp_mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    rdp_gateway: Option<String>,
    rdp_redirect_printers: bool,
    rdp_redirect_com_ports: bool,
    rdp_redirect_smart_cards: bool,
    rdp_redirect_webcam: bool,
    rdp_redirect_usb: bool,
    rdp_audio_capture: bool,
    rdp_multi_monitor: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rdp_selected_monitor_indices: Vec<u32>,
    rdp_strict_server_authentication: bool,
    rdp_disable_udp: bool,
    rdp_bitmap_caching: bool,
    rdp_compression: bool,
    rdp_hardware_acceleration: bool,
    rdp_full_screen: bool,
}

/// A post-connect step as the C# writes it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    input: String,
    delay_ms: u32,
    enabled: bool,
    on_failure: i64,
}

/// An SSH gateway as the C# writes it, without its secrets.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Gateway {
    id: String,
    name: String,
    host: String,
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_gateway_id: Option<String>,
}

/// The profiles and gateways of `store` as the C# session document, indented. A local
/// shell's arguments are written as `windows_line` makes them one Windows argument string:
/// the quoting of the terminal that runs them, which this crate does not hold.
///
/// # Panics
///
/// Never: the document holds only strings, numbers and booleans, which always serialize.
#[must_use]
pub fn csharp(
    store: &ProfileStore,
    windows_line: &dyn Fn(&LocalArguments) -> String,
    rdp_defaults: &RdpDefaults,
) -> String {
    let servers = store
        .ssh_profiles()
        .iter()
        .map(ssh)
        // A profile following the application's options is written with the ones in effect.
        .chain(
            store
                .rdp_profiles()
                .iter()
                .map(|profile| rdp(&profile.clone().effective(rdp_defaults))),
        )
        .chain(store.telnet_profiles().iter().map(telnet))
        .chain(store.vnc_profiles().iter().map(vnc))
        .chain(store.ftp_profiles().iter().map(ftp))
        .chain(store.citrix_profiles().iter().map(citrix))
        .chain(
            store
                .local_profiles()
                .iter()
                .map(|profile| local(profile, windows_line)),
        )
        .chain(store.winrm_profiles().iter().map(winrm))
        .map(|server| with_metadata(server, store))
        .collect();
    let document = Document {
        schema_version: SCHEMA_VERSION,
        servers,
        gateways: store.gateways().iter().map(gateway).collect(),
    };
    serde_json::to_string_pretty(&document).expect("a document of plain values serializes")
}

/// How many profiles [`csharp`] writes: the C# "{0} session(s) exported".
#[must_use]
pub fn session_count(store: &ProfileStore) -> usize {
    store.ssh_profiles().len()
        + store.rdp_profiles().len()
        + store.telnet_profiles().len()
        + store.vnc_profiles().len()
        + store.ftp_profiles().len()
        + store.citrix_profiles().len()
        + store.local_profiles().len()
        + store.winrm_profiles().len()
}

/// The keys every profile has.
fn server(
    id: &crate::profile::ProfileId,
    name: &str,
    group: Option<&String>,
    host: &str,
    connection_type: &'static str,
) -> Entry {
    Entry {
        id: id.as_str().to_owned(),
        display_name: name.to_owned(),
        remote_server: host.to_owned(),
        connection_type,
        group: group.cloned(),
        ..Entry::default()
    }
}

/// `server` with what its profile says of it, as the C# Metadata section keeps it.
fn with_metadata(mut server: Entry, store: &ProfileStore) -> Entry {
    if let Some(metadata) = store.metadata(&crate::profile::ProfileId::new(server.id.clone())) {
        server.environment = metadata.environment.map(|e| e.name().to_owned());
        server.tags = Some(metadata.tags.clone()).filter(|tags| !tags.is_empty());
        server.mac_address = metadata.mac_address.map(|mac| mac.to_string());
        server.origin = metadata.origin.map(ProfileOrigin::csharp_number);
        server.sort_order = metadata.sort_order;
        server.tunnels_panel_expanded = metadata.tunnels_expanded;
    }
    server
}

/// The ports opened through a gateway: 0, absent here, opens none.
fn with_forwards(mut server: Entry, forwards: &Forwards) -> Entry {
    server.socks_proxy_port = forwards.socks_port;
    server.remote_bind_port = forwards.remote_bind_port;
    server.remote_local_port = forwards.remote_local_port;
    server
}

fn ssh(profile: &SshProfile) -> Entry {
    let kind = if profile.sftp {
        connection_type::SFTP
    } else {
        connection_type::SSH
    };
    let server = Entry {
        vault_entry_name: profile.vault_entry.clone(),
        ssh_gateway_id: profile.gateway.as_ref().map(|id| id.as_str().to_owned()),
        ssh_port: Some(profile.port),
        ssh_username: profile.username.clone(),
        ssh_key_path: profile
            .key_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        ssh_agent_forwarding: profile.forward_agent,
        ssh_compression: profile.compression,
        ssh_mode: (profile.ssh_mode == SshMode::External).then_some("External"),
        ssh_x11_forwarding: profile.x11_forwarding,
        session_logging_override: profile.session_logging,
        post_connect_steps: profile.post_connect.steps.iter().map(step).collect(),
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            kind,
        )
    };
    with_forwards(server, &profile.forwards)
}

fn step(step: &PostConnectStep) -> Step {
    Step {
        input: step.input.clone(),
        delay_ms: step.delay_ms,
        enabled: step.enabled,
        on_failure: match step.on_failure {
            OnFailure::Continue => CONTINUE_POLICY,
            OnFailure::Stop => STOP_POLICY,
        },
    }
}

fn rdp(profile: &RdpProfile) -> Entry {
    let options = &profile.options;
    let server = Entry {
        vault_entry_name: profile.vault_entry.clone(),
        ssh_gateway_id: profile.gateway.as_ref().map(|id| id.as_str().to_owned()),
        rdp: Some(RdpKeys {
            remote_port: profile.port,
            rdp_username: profile.username.clone(),
            rdp_domain: profile.domain.clone(),
            use_direct_connection: profile.gateway.is_none(),
            rdp_use_global_defaults: false,
            rdp_nla: !profile.allow_tls_only,
            rdp_redirect_clipboard: profile.redirect_clipboard,
            rdp_redirect_drives: profile.redirect_drives,
            rdp_color_depth: options.color_depth.bits(),
            rdp_audio_mode: match options.audio {
                AudioPlayback::Off => AUDIO_OFF,
                AudioPlayback::Local => AUDIO_LOCAL,
                AudioPlayback::OnServer => AUDIO_ON_SERVER,
            },
            rdp_admin_mode: options.admin_session,
            rdp_resolution_mode: match options.resolution {
                Resolution::FitWindow => "FitWindow",
                Resolution::Fixed => "Fixed",
                Resolution::SmartSizing => "SmartSizing",
                Resolution::MultiMonitor => "Multimon",
                Resolution::Auto => "Auto",
            },
            rdp_fixed_width: options.fixed_width,
            rdp_fixed_height: options.fixed_height,
            rdp_initial_smart_sizing: options.scale_fixed,
            rdp_dynamic_resolution: options.dynamic_resolution,
            rdp_anti_idle: profile.anti_idle,
            rdp_auto_reconnect: profile.auto_reconnect,
            rdp_performance_flags: options.performance_flags,
            rdp_aspect_ratio: match options.aspect {
                Aspect::Stretch => "Stretch",
                Aspect::Wide => "16:9",
                Aspect::Standard => "4:3",
                Aspect::UltraWide => "21:9",
            },
            rdp_mode: if profile.extras.external {
                "External"
            } else {
                "Embedded"
            },
            rdp_gateway: profile.extras.rd_gateway().map(str::to_owned),
            rdp_redirect_printers: profile.extras.redirect_printers,
            rdp_redirect_com_ports: profile.extras.redirect_com_ports,
            rdp_redirect_smart_cards: profile.extras.redirect_smart_cards,
            rdp_redirect_webcam: profile.extras.redirect_webcam,
            rdp_redirect_usb: profile.extras.redirect_usb,
            rdp_audio_capture: profile.extras.microphone,
            rdp_multi_monitor: profile.extras.multi_monitor,
            rdp_selected_monitor_indices: profile.extras.monitors.clone(),
            rdp_strict_server_authentication: profile.extras.strict_server_authentication,
            rdp_disable_udp: profile.extras.disable_udp,
            rdp_bitmap_caching: profile.extras.bitmap_caching,
            rdp_compression: profile.extras.compression,
            rdp_hardware_acceleration: profile.extras.hardware_acceleration,
            rdp_full_screen: profile.extras.full_screen,
        }),
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            connection_type::RDP,
        )
    };
    with_forwards(server, &profile.forwards)
}

fn telnet(profile: &TelnetProfile) -> Entry {
    Entry {
        telnet_port: Some(profile.port),
        session_logging_override: profile.session_logging,
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            connection_type::TELNET,
        )
    }
}

fn vnc(profile: &VncProfile) -> Entry {
    Entry {
        vault_entry_name: profile.vault_entry.clone(),
        vnc_port: Some(profile.port),
        vnc_view_only: profile.view_only,
        vnc_allow_no_password: Some(profile.allow_no_password),
        vnc_require_tls: Some(profile.require_tls),
        vnc_username: profile.username.clone(),
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            connection_type::VNC,
        )
    }
}

fn ftp(profile: &FtpProfile) -> Entry {
    Entry {
        vault_entry_name: profile.vault_entry.clone(),
        ftp_port: Some(profile.port),
        ftp_username: profile.username.clone(),
        ftp_passive_mode: Some(profile.passive),
        ftp_use_ssl: profile.tls,
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            connection_type::FTP,
        )
    }
}

/// A Citrix application: never its Workspace cache launch line, a secret the C# export
/// leaves out too.
fn citrix(profile: &CitrixProfile) -> Entry {
    Entry {
        citrix_store_front_url: profile.store_front_url.clone(),
        citrix_app_name: profile.app_name.clone(),
        citrix_ica_file_path: profile.ica_file.clone(),
        citrix_seamless_mode: Some(profile.seamless),
        citrix_use_sso: Some(profile.sso),
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            "",
            connection_type::CITRIX,
        )
    }
}

/// A local shell: never its approval, which the reader gives again, as the C# confirms a
/// command it has not run yet.
fn local(profile: &LocalProfile, windows_line: &dyn Fn(&LocalArguments) -> String) -> Entry {
    let command = &profile.command;
    let arguments = windows_line(&command.arguments);
    Entry {
        local_shell_executable: command.program.clone(),
        local_shell_arguments: (!arguments.is_empty()).then_some(arguments),
        local_shell_working_directory: command
            .working_directory
            .as_ref()
            .map(|folder| folder.to_string_lossy().into_owned()),
        elevation_mode: command.run_as_administrator.then_some(RUNAS_ELEVATION),
        session_logging_override: profile.session_logging,
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            "",
            connection_type::LOCAL,
        )
    }
}

fn winrm(profile: &WinRmProfile) -> Entry {
    Entry {
        ssh_gateway_id: profile.gateway.as_ref().map(|id| id.as_str().to_owned()),
        win_rm_port: Some(profile.port),
        win_rm_username: profile.username.clone(),
        win_rm_use_ssl: profile.use_ssl,
        win_rm_skip_certificate_check: profile.skip_certificate_check,
        win_rm_identity_mode: Some(if profile.username.is_some() {
            WINRM_CREDENTIAL
        } else {
            WINRM_CURRENT_USER
        }),
        ..server(
            &profile.id,
            &profile.name,
            profile.group.as_ref(),
            &profile.host,
            connection_type::WINRM,
        )
    }
}

fn gateway(gateway: &SshGateway) -> Gateway {
    Gateway {
        id: gateway.id.as_str().to_owned(),
        name: gateway.name.clone(),
        host: gateway.host.clone(),
        port: gateway.port,
        user: gateway.username.clone(),
        key_path: gateway
            .key_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        parent_gateway_id: gateway.parent.as_ref().map(|id| id.as_str().to_owned()),
    }
}
