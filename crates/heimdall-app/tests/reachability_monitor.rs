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

//! The background check of every server, as the C# session health monitor.

use std::path::Path;
use std::time::Duration;

use heimdall_app::reachability::{DownReason, Probe, Unchecked, Verdict, check};
use heimdall_app::{App, AppConfig, Effect, Message, SettingsMessage};
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str, gateway: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: gateway.map(ProfileId::new),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
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

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([ssh("web", None), ssh("db", Some("bastion"))]);
    store.merge_local([LocalProfile {
        id: ProfileId::new("tool"),
        name: "tool".to_owned(),
        group: None,
        command: LocalCommand {
            program: Some("tool".to_owned()),
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
            run_as_administrator: false,
        },
        approved: None,
        session_logging: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn verdict(app: &App, id: &str) -> Option<Verdict> {
    app.reachability(&ProfileId::new(id)).cloned()
}

fn probes(effects: &[Effect]) -> Vec<Probe> {
    match effects {
        [Effect::CheckReachability { probes, .. }] => probes.clone(),
        [] => Vec::new(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_round_dials_only_the_servers_this_computer_reaches_and_keeps_the_last_word() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(
        app.reachability_interval(),
        Some(Duration::from_secs(60)),
        "on, every minute, as the C#"
    );

    // The first round: the direct server dialled, the others said why not.
    let first = app.start_reachability();
    assert!(matches!(
        first.as_slice(),
        [Effect::CheckReachability { timeout, at_once: 10, .. }]
            if *timeout == Duration::from_secs(2)
    ));
    assert_eq!(
        probes(&first),
        [Probe {
            id: ProfileId::new("web"),
            host: "web.lab".to_owned(),
            port: 22,
        }]
    );
    assert_eq!(verdict(&app, "web"), Some(Verdict::Checking));
    assert_eq!(
        verdict(&app, "db"),
        Some(Verdict::Unchecked(Unchecked::BehindGateway))
    );
    assert_eq!(
        verdict(&app, "tool"),
        Some(Verdict::Unchecked(Unchecked::NoPort))
    );
    assert!(app.start_reachability().is_empty(), "started once");

    // A round still running is not started again.
    assert!(app.update(Message::ReachabilityTick).is_empty());
    app.update(Message::ReachabilityChecked {
        id: ProfileId::new("web"),
        verdict: Verdict::Up(12),
    });
    assert_eq!(verdict(&app, "web"), Some(Verdict::Up(12)));

    // The next round: what it said stays while it is dialled again.
    let next = app.update(Message::ReachabilityTick);
    assert_eq!(probes(&next).len(), 1);
    assert_eq!(verdict(&app, "web"), Some(Verdict::Up(12)));
}

#[test]
fn turned_off_it_forgets_and_turned_on_it_looks_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let _ = app.start_reachability();
    app.update(Message::ReachabilityChecked {
        id: ProfileId::new("web"),
        verdict: Verdict::Down(DownReason::Refused),
    });

    app.update(Message::Settings(SettingsMessage::Reachability(false)));
    assert_eq!(app.reachability_interval(), None);
    assert_eq!(verdict(&app, "web"), None, "nothing shown while off");
    assert!(app.update(Message::ReachabilityTick).is_empty());
    // An answer arriving after: still nothing.
    app.update(Message::ReachabilityChecked {
        id: ProfileId::new("web"),
        verdict: Verdict::Up(3),
    });
    assert_eq!(verdict(&app, "web"), None);

    let on = app.update(Message::Settings(SettingsMessage::Reachability(true)));
    assert_eq!(probes(&on).len(), 1, "a round at once");

    // Its numbers, within the C# ranges only.
    app.update(Message::Settings(SettingsMessage::ReachabilityInterval(5)));
    app.update(Message::Settings(SettingsMessage::ReachabilityInterval(
        300,
    )));
    assert_eq!(app.reachability_interval(), Some(Duration::from_secs(300)));
    app.update(Message::Settings(SettingsMessage::ReachabilityTimeout(100)));
    app.update(Message::Settings(SettingsMessage::ReachabilityProbes(51)));
    assert_eq!(app.settings().reachability.timeout, 2000);
    assert_eq!(app.settings().reachability.probes, 10);
}

#[tokio::test]
async fn a_listening_port_is_up_and_a_name_not_found_is_said_so() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let up = check("127.0.0.1".to_owned(), port, Duration::from_secs(2)).await;
    assert!(matches!(up, Verdict::Up(millis) if millis >= 1), "{up:?}");

    let dns = check(
        "no-such-host.invalid".to_owned(),
        22,
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(dns, Verdict::Down(DownReason::Dns));
}
