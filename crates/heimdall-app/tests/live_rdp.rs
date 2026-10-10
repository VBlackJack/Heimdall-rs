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

//! The RDP attempt against a real server, opt-in: `HEIMDALL_LIVE_RDP_PORT` names the port
//! of the Heimdall-TestEnv xrdp image. That server has no Network Level Authentication,
//! which the application requires: the attempt must report the refusal plainly, without
//! asking for a password first.

use std::time::Duration;

use heimdall_app::rdp_driver::{DEFAULT_DESKTOP, RdpRequest, rdp_events};
use heimdall_app::{AnswerRegistry, ConnectionEvent, UiError};
use heimdall_core::profile::{ProfileId, RdpProfile};
use tokio_stream::StreamExt as _;
use tokio_util::sync::CancellationToken;

const PORT_VARIABLE: &str = "HEIMDALL_LIVE_RDP_PORT";

/// Bound on each event.
const STEP: Duration = Duration::from_secs(30);

#[tokio::test]
async fn a_server_without_nla_is_refused_before_any_password_question() {
    let Some(port) = std::env::var(PORT_VARIABLE)
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
    else {
        eprintln!("{PORT_VARIABLE} is not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let mut events = rdp_events(
        RdpRequest {
            profile: RdpProfile {
                extras: heimdall_core::profile::RdpExtras::default(),
                id: ProfileId::new("xrdp"),
                name: "xrdp".to_owned(),
                group: None,
                host: "127.0.0.1".to_owned(),
                port,
                username: Some("nobody".to_owned()),
                domain: None,
                allow_tls_only: false,
                gateway: None,
                local_tunnel_port: None,
                redirect_clipboard: true,
                redirect_drives: false,
                options: heimdall_core::profile::RdpOptions::default(),
                vault_entry: None,
                forwards: heimdall_core::profile::Forwards::default(),
                follow_defaults: false,
                several_servers: false,
                anti_idle: false,
                auto_reconnect: true,
            },
            known_hosts: dir.path().join("known_rdp_hosts"),
            accepted: None,
            trusted_for_run: Vec::new(),
            desktop: DEFAULT_DESKTOP,
            desktop_scale: 100,
            logon_timeout: None,
            route: Vec::new(),
            ssh: heimdall_ssh::ConnectOptions::new(dir.path().join("known_hosts")),
            cancel: CancellationToken::new(),
            credential_guard: None,
        },
        registry.clone(),
    );
    let first = tokio::time::timeout(STEP, events.next())
        .await
        .expect("in time")
        .expect("an event");
    assert!(
        matches!(
            first,
            ConnectionEvent::Failed(UiError::SecurityRefused { .. })
        ),
        "{first:?}"
    );
    assert_eq!(
        registry.pending(),
        0,
        "no password asked for a refused server"
    );
}
