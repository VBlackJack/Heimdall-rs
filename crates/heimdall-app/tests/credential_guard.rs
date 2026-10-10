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

//! Credential Guard required before an embedded RDP session, as the C# gate: an embedded
//! desktop waits for the check, opens while Credential Guard runs and is refused otherwise,
//! said once for a batch, before Windows Hello is asked. Remote Desktop Connection, an RD
//! Gateway profile included, is not concerned. The answers are given here: `PowerShell`
//! never runs.

use std::path::Path;

use heimdall_app::credential_guard::{Failure, Status};
use heimdall_app::windows_hello::HelloRefusal;
use heimdall_app::{
    Announcement, App, AppConfig, ConnectionEvent, Effect, Message, Notice, QuickResult,
    SelectionMessage, SettingsMessage, SystemCredentials, TabId, UiError,
};
use heimdall_core::profile::{
    Forwards, ProfileId, RdpExtras, RdpMode, RdpOptions, RdpProfile, SshProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn rdp(id: &str, external: bool, rd_gateway: Option<&str>) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras {
            external,
            rd_gateway: rd_gateway.map(str::to_owned),
            ..RdpExtras::default()
        },
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: RdpOptions::default(),
        vault_entry: None,
        forwards: Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

fn ssh(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// `a` and `c` open in a tab, `b` is SSH, `mstsc` opens in Remote Desktop Connection and
/// `rdg` names an RD Gateway, which opens there too.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([ssh("b")]);
    store.merge_rdp([
        rdp("a", false, None),
        rdp("c", false, None),
        rdp("mstsc", true, None),
        rdp("rdg", false, Some("rdg.example")),
    ]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

/// The application with Credential Guard required; the background check its switch asks
/// for is left unanswered.
fn required(dir: &Path) -> App {
    let mut app = app(dir);
    let effects = app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
        true,
    )));
    assert!(checks(&effects), "turned on, checked: {effects:?}");
    app
}

/// The application with Credential Guard required and its answer `status` known.
fn answered(dir: &Path, status: Status) -> App {
    let mut app = required(dir);
    assert!(app.update(Message::CredentialGuard(status)).is_empty());
    app
}

fn open(app: &mut App, id: &str) -> Vec<Effect> {
    app.update(Message::OpenRdp(ProfileId::new(id)))
}

fn checks(effects: &[Effect]) -> bool {
    matches!(effects, [Effect::CheckCredentialGuard(_)])
}

/// The tabs the RDP attempts of `effects` open, each with whether it carries the detection.
fn connects(effects: &[Effect]) -> Vec<(TabId, bool)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ConnectRdp { tab, request, .. } => {
                Some((*tab, request.credential_guard.is_some()))
            }
            _ => None,
        })
        .collect()
}

fn launches_mstsc(effects: &[Effect]) -> bool {
    matches!(effects, [Effect::LaunchRdpExternal { .. }])
}

/// How many times the refusal was announced.
fn refusals(app: &App) -> usize {
    app.announcements()
        .filter(|announced| announced.what == Announcement::Notice(Notice::CredentialGuardRequired))
        .count()
}

#[test]
fn off_by_default_an_embedded_session_opens_unchecked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(!app.settings().require_credential_guard);
    assert!(
        app.warm_credential_guard().is_empty(),
        "nothing checked at start"
    );
    let opened = connects(&open(&mut app, "a"));
    assert_eq!(opened.len(), 1);
    assert!(!opened[0].1, "the attempt does not ask either");
}

#[test]
fn a_cold_answer_holds_every_embedded_session_for_one_check_then_lets_them_through() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // Turned on with a session waiting already: no second check.
    app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
        true,
    )));
    assert!(
        open(&mut app, "a").is_empty(),
        "waits for the check running"
    );
    assert!(app.tabs.is_empty(), "nothing opens before the answer");
    assert!(open(&mut app, "c").is_empty(), "waits for the same one");
    let effects = app.update(Message::CredentialGuard(Status::Active));
    let opened = connects(&effects);
    assert_eq!(opened.len(), 2, "{effects:?}");
    assert!(
        opened.iter().all(|(_, asks)| *asks),
        "each attempt asks again"
    );
    assert_eq!(app.credential_guard_status(), Some(Status::Active));
    // Kept: the next one opens at once.
    assert_eq!(connects(&open(&mut app, "a")).len(), 1);
}

#[test]
fn a_session_opened_before_any_check_starts_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = answered(dir.path(), Status::Indeterminate(Failure::TimedOut));
    assert_eq!(
        app.credential_guard_status(),
        Some(Status::Indeterminate(Failure::TimedOut)),
        "said in Settings, why included"
    );
    // Not kept: the next session asks again.
    assert!(checks(&open(&mut app, "a")));
    assert!(open(&mut app, "c").is_empty(), "one check at a time");
    let effects = app.update(Message::CredentialGuard(Status::Active));
    assert_eq!(connects(&effects).len(), 2, "{effects:?}");
}

#[test]
fn not_running_or_unknown_refuses_says_it_once_and_only_a_definitive_answer_is_kept() {
    for status in [
        Status::Inactive,
        Status::Indeterminate(Failure::NotStarted("blocked by policy".to_owned())),
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app(dir.path());
        app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
            true,
        )));
        assert!(open(&mut app, "a").is_empty());
        assert!(open(&mut app, "c").is_empty());
        let effects = app.update(Message::CredentialGuard(status.clone()));
        assert!(effects.is_empty(), "{status:?}: {effects:?}");
        assert!(app.tabs.is_empty(), "{status:?}: fail closed");
        assert_eq!(app.notice(), Some(&Notice::CredentialGuardRequired));
        assert_eq!(refusals(&app), 1, "{status:?}: said once for both");
        let again = open(&mut app, "a");
        if status.is_definitive() {
            assert!(again.is_empty(), "known: refused at once, nothing asked");
            assert!(app.tabs.is_empty());
        } else {
            assert!(checks(&again), "not kept: checked again");
        }
    }
}

#[test]
fn credential_guard_is_asked_before_windows_hello_which_a_refusal_never_raises() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = required(dir.path());
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(open(&mut app, "a").is_empty(), "the check runs: no prompt");
    let effects = app.update(Message::CredentialGuard(Status::Inactive));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::VerifyWindowsHello)),
        "{effects:?}"
    );
    assert!(effects.is_empty());
    assert_eq!(app.notice(), Some(&Notice::CredentialGuardRequired));
    // Known not to run: refused at once, still without the prompt.
    assert!(open(&mut app, "a").is_empty());

    // Running: Windows Hello is asked next, then the session opens.
    let dir = tempfile::tempdir().expect("dir");
    let mut app = required(dir.path());
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(open(&mut app, "a").is_empty());
    let effects = app.update(Message::CredentialGuard(Status::Active));
    assert!(
        matches!(effects.as_slice(), [Effect::VerifyWindowsHello]),
        "{effects:?}"
    );
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert_eq!(connects(&effects).len(), 1, "{effects:?}");

    // Running, then Windows Hello refused: its own refusal, not this one.
    let dir = tempfile::tempdir().expect("dir");
    let mut app = answered(dir.path(), Status::Active);
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(matches!(
        open(&mut app, "a").as_slice(),
        [Effect::VerifyWindowsHello]
    ));
    app.update(Message::WindowsHello(Err(HelloRefusal::Cancelled)));
    assert_eq!(
        app.notice(),
        Some(&Notice::WindowsHelloRefused(HelloRefusal::Cancelled))
    );
    assert_eq!(refusals(&app), 0);
}

#[test]
fn a_batch_is_refused_with_one_notice_whatever_opens_between() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = answered(dir.path(), Status::Inactive);
    app.update(Message::SelectProfile(ProfileId::new("a")));
    for id in ["b", "c"] {
        app.update(Message::Selection(SelectionMessage::Toggle(
            ProfileId::new(id),
        )));
    }
    let effects = app.update(Message::Selection(SelectionMessage::Connect));
    assert!(connects(&effects).is_empty(), "{effects:?}");
    assert_eq!(app.tabs.len(), 1, "the SSH session opens, alone");
    assert_eq!(refusals(&app), 1, "one notice for the batch");
    // The next batch says it again.
    app.update(Message::Selection(SelectionMessage::Connect));
    assert_eq!(refusals(&app), 2);
}

#[test]
fn remote_desktop_connection_and_an_rd_gateway_are_not_held() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = answered(dir.path(), Status::Inactive);
    assert!(launches_mstsc(&open(&mut app, "mstsc")));
    assert!(launches_mstsc(&open(&mut app, "rdg")), "opens in mstsc");
    assert!(launches_mstsc(&app.update(Message::OpenRdpWith {
        id: ProfileId::new("a"),
        mode: RdpMode::External,
    })));
    assert_eq!(refusals(&app), 0);
    // An external profile opened in a tab this once is held to it.
    assert!(
        app.update(Message::OpenRdpWith {
            id: ProfileId::new("mstsc"),
            mode: RdpMode::Embedded,
        })
        .is_empty()
    );
    assert_eq!(refusals(&app), 1);
}

/// A desktop of `a` opened then failed with `error`, before the setting was on.
fn failed_tab(app: &mut App, error: UiError) -> TabId {
    let effects = open(app, "a");
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    app.update(Message::Connection {
        tab: *tab,
        attempt: *attempt,
        event: ConnectionEvent::Failed(error),
    });
    *tab
}

#[test]
fn a_reconnect_waits_then_opens_in_its_place_carrying_the_detection() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = failed_tab(&mut app, UiError::Timeout);
    assert!(checks(&app.update(Message::Settings(
        SettingsMessage::RequireCredentialGuard(true)
    ))));
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());
    let effects = app.update(Message::CredentialGuard(Status::Active));
    let [(again, true)] = connects(&effects)[..] else {
        panic!("{effects:?}");
    };
    assert_eq!(app.tabs.len(), 1, "in its place");
    assert_eq!(app.tabs[0].id, again);

    // Refused: the tab stays as it was.
    let dir = tempfile::tempdir().expect("dir");
    let mut app = self::app(dir.path());
    let tab = failed_tab(&mut app, UiError::Timeout);
    app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
        true,
    )));
    app.update(Message::CredentialGuard(Status::Inactive));
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(refusals(&app), 1);
}

#[test]
fn an_auto_reconnect_refused_stops_its_chain() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = open(&mut app, "a");
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    // Live, then dropped: the chain starts.
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: heimdall_rdp::Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    });
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    assert!(app.tab(tab).expect("tab").retry.is_some(), "waiting");
    app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
        true,
    )));
    assert!(
        app.update(Message::AutoReconnect { tab, attempt })
            .is_empty(),
        "held for the check running"
    );
    let effects = app.update(Message::CredentialGuard(Status::Inactive));
    assert!(connects(&effects).is_empty(), "{effects:?}");
    assert_eq!(
        app.tab(tab).expect("tab").retry,
        None,
        "the chain stops: Reconnect is the user's"
    );
}

#[test]
fn the_check_runs_at_start_and_when_turned_on_never_twice_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(checks(&app.update(Message::Settings(
        SettingsMessage::RequireCredentialGuard(true)
    ))));
    assert!(app.warm_credential_guard().is_empty(), "running already");
    // Off then on again while it runs: still one.
    app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
        false,
    )));
    assert!(
        app.update(Message::Settings(SettingsMessage::RequireCredentialGuard(
            true
        )))
        .is_empty()
    );
    app.update(Message::CredentialGuard(Status::Active));
    assert!(app.warm_credential_guard().is_empty(), "known: never again");

    // A later start with the setting saved on checks in the background.
    let mut again = app_in(dir.path());
    assert!(again.settings().require_credential_guard);
    assert!(checks(&again.warm_credential_guard()));
    assert_eq!(again.credential_guard_status(), None, "nothing known yet");
}

/// The application over the files `app` left in `dir`.
fn app_in(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

#[tokio::test]
async fn a_desktop_saved_nowhere_is_refused_by_its_attempt() {
    use tokio_stream::StreamExt as _;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = answered(dir.path(), Status::Inactive);
    let effects = app.update(Message::QuickConnect(QuickResult::Rdp {
        host: "adhoc.invalid".to_owned(),
    }));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(request.credential_guard.is_some(), "the attempt asks");
    let mut events = heimdall_app::rdp_driver::rdp_events(
        (**request).clone(),
        heimdall_app::AnswerRegistry::default(),
    );
    assert!(matches!(
        events.next().await,
        Some(ConnectionEvent::Failed(UiError::CredentialGuardRequired))
    ));
}
