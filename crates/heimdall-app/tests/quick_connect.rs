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

//! Quick Connect, as the C# Heimdall's Ctrl+K palette: the sessions it finds, and a host
//! typed that no session matches opened as a session never saved.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, Message, Purpose, QuickResult, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str, name: &str, host: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: group.map(str::to_owned),
        host: host.to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
    }
}

fn app_with(dir: &Path, profiles: Vec<SshProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(profiles);
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: Some("Windows".to_owned()),
        host: "dc01.corp".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
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

fn app(dir: &Path) -> App {
    app_with(
        dir,
        vec![
            ssh("web", "webserver", "10.0.0.8", Some("Production")),
            ssh("mail", "Mail", "my-web.lab", None),
            ssh("db", "Database", "db.lab", Some("Production")),
        ],
    )
}

fn names(results: &[QuickResult]) -> Vec<String> {
    results
        .iter()
        .map(|result| match result {
            QuickResult::Profile(profile) => profile.name.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn nothing_typed_shows_the_first_sessions_by_name() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(
        names(&app.quick_results("  ")),
        ["Database", "Domain controller", "Mail", "webserver"]
    );

    let many = (0..12)
        .map(|n| ssh(&format!("s{n:02}"), &format!("s{n:02}"), "h.lab", None))
        .collect();
    let dir = tempfile::tempdir().expect("dir");
    let app = app_with(dir.path(), many);
    let first = app.quick_results("");
    assert_eq!(first.len(), 10, "the first ten, as the C# palette");
    assert_eq!(names(&first)[0], "Domain controller");
    assert_eq!(names(&first)[9], "s08");
}

#[test]
fn a_name_starting_with_the_search_comes_before_one_holding_it() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    // "webserver" starts with it; Mail's host holds it.
    assert_eq!(names(&app.quick_results("web")), ["webserver", "Mail"]);
    // The host is searched as the name; the folder by half.
    assert_eq!(names(&app.quick_results("dc01")), ["Domain controller"]);
    assert_eq!(
        names(&app.quick_results("prod")),
        ["Database", "webserver"],
        "equal scores by name"
    );
    // The account and the protocol too.
    assert_eq!(
        names(&app.quick_results("RDP")),
        ["Domain controller"],
        "by its protocol"
    );
    assert_eq!(app.quick_results("admin").len(), 3, "by the account");
}

#[test]
fn at_most_twenty_sessions_are_found() {
    let many = (0..25)
        .map(|n| ssh(&format!("s{n:02}"), &format!("node{n:02}"), "h.lab", None))
        .collect();
    let dir = tempfile::tempdir().expect("dir");
    let app = app_with(dir.path(), many);
    let found = app.quick_results("node");
    assert_eq!(found.len(), 20);
    assert_eq!(names(&found)[19], "node19");
}

#[test]
fn a_host_no_session_matches_is_offered_as_ssh_and_rdp() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(
        app.quick_results(" jump.lab "),
        [
            QuickResult::Ssh {
                username: None,
                host: "jump.lab".to_owned(),
                port: 22
            },
            QuickResult::Rdp {
                host: "jump.lab".to_owned()
            }
        ]
    );
    assert_eq!(
        app.quick_results("ssh root@jump.lab:2222"),
        [QuickResult::Ssh {
            username: Some("root".to_owned()),
            host: "jump.lab".to_owned(),
            port: 2222
        }]
    );
    assert!(
        app.quick_results("no such thing").is_empty(),
        "neither a session nor a host"
    );
    assert!(
        !app.quick_results("10.0.0.8")
            .iter()
            .any(|result| !matches!(result, QuickResult::Profile(_))),
        "a host a session matches opens that session"
    );
}

#[test]
fn a_session_found_opens_as_a_click_in_the_tree() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let found = app.quick_results("database").remove(0);
    let effects = app.update(Message::QuickConnect(found));
    let [Effect::Connect { tab, request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.id.as_str(), "db");
    let opened = app.tab(*tab).expect("tab");
    assert!(app.tab_profile(opened).is_some(), "the saved one");
}

#[test]
fn a_host_typed_opens_a_session_never_saved_that_reconnects_as_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::QuickConnect(QuickResult::Ssh {
        username: Some("root".to_owned()),
        host: "jump.lab".to_owned(),
        port: 2222,
    }));
    let [
        Effect::Connect {
            tab,
            attempt,
            request,
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    assert_eq!(
        (
            request.profile.host.as_str(),
            request.profile.port,
            request.profile.username.as_deref()
        ),
        ("jump.lab", 2222, Some("root"))
    );
    assert_eq!(request.purpose, Purpose::Shell);
    assert!(request.route.is_empty());
    let opened = app.tab(tab).expect("tab");
    assert_eq!(opened.display_title(), "jump.lab");
    assert!(app.tab_profile(opened).is_none(), "saved nowhere");
    assert_eq!(app.profile_summaries().len(), 4, "nothing added");

    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    let effects = app.update(Message::ReconnectTab(tab));
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("jump.lab", 2222),
        "again as it was"
    );

    let effects = app.update(Message::QuickConnect(QuickResult::Rdp {
        host: "ts.lab".to_owned(),
    }));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("ts.lab", 3389)
    );
    assert!(!request.profile.allow_tls_only && request.profile.redirect_clipboard);
    assert!(!request.profile.redirect_drives);
}
