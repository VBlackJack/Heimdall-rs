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

//! The stored password of a `WinRM` profile, as the C# Heimdall's stored credential: given
//! only for the profile's own host, port and account, never a gateway's forward, once an
//! attempt; a session given it that is never entered takes it as refused, and `PowerShell`
//! asks from then on.

#[path = "support/log_capture.rs"]
mod log_capture;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, SystemCredentials, TabId, UiError,
};
use heimdall_core::credentials::{
    CredentialProtocol, Endpoint, SavedPassword, encode, password_entry,
};
use heimdall_core::profile::{DEFAULT_WINRM_HTTP_PORT, ProfileId, SshGateway, WinRmProfile};
use heimdall_core::store::ProfileStore;
use heimdall_core::winrm::{self, PasswordSource, REMOTE_SESSION_NOT_ENTERED_EXIT_CODE};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use zeroize::Zeroizing;

const PASSWORD: &str = "st0red winrm p4ss";

/// A loopback forward a gateway might open: never the server a password is for.
const FORWARD_PORT: u16 = 50123;

fn id(value: &str) -> ProfileId {
    ProfileId::new(value)
}

fn bastion() -> SshGateway {
    SshGateway {
        id: id("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.example.org".to_owned(),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }
}

fn profile(name: &str, username: Option<&str>, gateway: Option<&str>) -> WinRmProfile {
    WinRmProfile {
        id: id(name),
        name: name.to_owned(),
        group: None,
        host: "dc01.lab".to_owned(),
        port: DEFAULT_WINRM_HTTP_PORT,
        use_ssl: false,
        skip_certificate_check: false,
        username: username.map(str::to_owned),
        gateway: gateway.map(id),
    }
}

fn app(dir: &Path, profiles: Vec<WinRmProfile>, system: &SystemCredentials) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(vec![bastion()]);
    store.merge_winrm(profiles);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

/// Saves `password` for `profile` at `endpoint`, as the editor or a bulk edit would.
fn store_password(system: &SystemCredentials, profile: &str, endpoint: Endpoint, password: &str) {
    let SystemCredentials::Memory(entries) = system else {
        unreachable!()
    };
    let saved = SavedPassword {
        endpoint,
        password: Zeroizing::new(password.to_owned()),
    };
    entries
        .lock()
        .expect("entries")
        .insert(password_entry(&id(profile)), encode(&saved));
}

fn endpoint(host: &str, port: u16, username: &str) -> Endpoint {
    Endpoint {
        protocol: CredentialProtocol::WinRm,
        host: host.to_owned(),
        port,
        username: Some(username.to_owned()),
    }
}

/// Opens `profile`: its tab, attempt and stored password.
fn open(app: &mut App, profile: &str) -> (TabId, AttemptId, Option<String>) {
    let effects = app.update(Message::OpenWinRm(id(profile)));
    let [
        Effect::ConnectWinRm {
            tab,
            attempt,
            request,
        },
    ] = effects.as_slice()
    else {
        panic!("expected ConnectWinRm, got {effects:?}");
    };
    let password = request
        .password
        .as_ref()
        .map(|password| password.expose().to_owned());
    (*tab, *attempt, password)
}

fn closed(app: &mut App, tab: TabId, attempt: AttemptId, exit_status: i32) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Closed {
            exit_status: Some(u32::try_from(exit_status).expect("an exit code")),
        },
    });
}

#[test]
fn the_password_is_given_for_the_servers_own_endpoint_only() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(
        dir.path(),
        vec![
            profile("direct", Some("LAB\\admin"), None),
            profile("routed", Some("LAB\\admin"), Some("bastion")),
            profile("forward", Some("LAB\\admin"), Some("bastion")),
            profile("current", None, None),
            profile("other", Some("LAB\\admin"), None),
        ],
        &system,
    );
    let server = || endpoint("DC01.LAB", DEFAULT_WINRM_HTTP_PORT, "LAB\\admin");
    store_password(&system, "direct", server(), PASSWORD);
    store_password(&system, "routed", server(), PASSWORD);
    // Saved for the forward a gateway opened: not the server's endpoint.
    store_password(
        &system,
        "forward",
        endpoint("127.0.0.1", FORWARD_PORT, "LAB\\admin"),
        PASSWORD,
    );
    store_password(&system, "current", server(), PASSWORD);
    let ssh = Endpoint {
        protocol: CredentialProtocol::Ssh,
        ..server()
    };
    store_password(&system, "other", ssh, PASSWORD);

    assert_eq!(open(&mut app, "direct").2.as_deref(), Some(PASSWORD));
    assert_eq!(
        open(&mut app, "routed").2.as_deref(),
        Some(PASSWORD),
        "through a gateway, the server's own host and port"
    );
    assert_eq!(open(&mut app, "forward").2, None, "never the forward's");
    assert_eq!(open(&mut app, "current").2, None, "no account, no password");
    assert_eq!(open(&mut app, "other").2, None, "another protocol's");
}

#[test]
fn a_password_refused_once_has_powershell_ask_until_a_new_one_is_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(
        dir.path(),
        vec![profile("w", Some("LAB\\admin"), None)],
        &system,
    );
    store_password(
        &system,
        "w",
        endpoint("dc01.lab", DEFAULT_WINRM_HTTP_PORT, "LAB\\admin"),
        PASSWORD,
    );
    log_capture::start();
    let (tab, attempt, password) = open(&mut app, "w");
    assert_eq!(password.as_deref(), Some(PASSWORD));
    // Never entered: the stored password was refused.
    closed(&mut app, tab, attempt, REMOTE_SESSION_NOT_ENTERED_EXIT_CODE);
    assert_eq!(
        app.tab(tab).expect("tab").winrm_diagnostic,
        Some(heimdall_core::winrm_diagnostic::Diagnostic::SessionNotEntered)
    );
    assert!(log_capture::has("INFO", &["stored password refused"]));
    assert!(log_capture::never(PASSWORD), "never in a log");

    // Next time PowerShell asks, by the account's name.
    let (_, _, password) = open(&mut app, "w");
    assert_eq!(password, None);
    let w = app.winrm_profiles()[0].clone();
    let command = winrm::session_command(&w, PasswordSource::Prompt).expect("valid");
    assert!(command.contains("-Credential 'LAB\\admin'"), "{command}");

    // A new password saved in the editor is given again.
    app.update(Message::EditProfile(id("w")));
    app.update(Message::SaveProfile {
        password: Some(Secret::new("new p4ss".to_owned())),
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(open(&mut app, "w").2.as_deref(), Some("new p4ss"));
}

#[test]
fn a_session_entered_or_never_started_keeps_its_password() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(
        dir.path(),
        vec![profile("w", Some("LAB\\admin"), None)],
        &system,
    );
    store_password(
        &system,
        "w",
        endpoint("dc01.lab", DEFAULT_WINRM_HTTP_PORT, "LAB\\admin"),
        PASSWORD,
    );
    // Entered, then ended.
    let (tab, attempt, _) = open(&mut app, "w");
    closed(
        &mut app,
        tab,
        attempt,
        winrm::REMOTE_SESSION_ENDED_EXIT_CODE,
    );
    // Failed before PowerShell ran: the password never left Heimdall.
    let (tab, failed, _) = open(&mut app, "w");
    app.update(Message::Connection {
        tab,
        attempt: failed,
        event: ConnectionEvent::Failed(UiError::LocalShell {
            detail: "unreachable".to_owned(),
        }),
    });
    // Another attempt's end: not the current one's.
    let (tab, _, _) = open(&mut app, "w");
    closed(&mut app, tab, failed, REMOTE_SESSION_NOT_ENTERED_EXIT_CODE);
    assert_eq!(open(&mut app, "w").2.as_deref(), Some(PASSWORD));
}
