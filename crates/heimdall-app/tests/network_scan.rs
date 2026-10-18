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

//! The network scanner dialog of Ctrl+Shift+N, as the C# `NetworkScannerService`: the
//! target typed and checked, the hosts a sweep tells of listed with the session their ports
//! suggest, a host already saved left out, and the hosts chosen added as sessions of the
//! right kind, in the discovered folder, with no account.

use std::net::Ipv4Addr;
use std::path::Path;

use heimdall_app::profile_draft::{DraftProfile, DraftProtocol, ProfileDraft, ProfileField};
use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, NetworkScan, NetworkScanMessage, SystemCredentials,
};
use heimdall_core::profile::ProfileId;
use heimdall_core::store::ProfileStore;
use heimdall_core::tools::network_scanner::{ScanEvent, ScannedHost, TargetError};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
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

fn send(app: &mut App, message: NetworkScanMessage) -> Vec<Effect> {
    app.update(Message::NetworkScan(message))
}

fn scan(app: &App) -> &NetworkScan {
    match &app.dialog {
        Some(Dialog::NetworkScan(scan)) => scan,
        other => panic!("no scanner: {other:?}"),
    }
}

fn host(last: u8, name: Option<&str>, ports: &[u16]) -> ScannedHost {
    ScannedHost {
        ip: Ipv4Addr::new(10, 0, 0, last),
        hostname: name.map(str::to_owned),
        round_trip_ms: 1,
        open_ports: ports.to_vec(),
    }
}

/// Starts a sweep of `target` and returns its number.
fn start(app: &mut App, target: &str) -> u64 {
    send(app, NetworkScanMessage::Open);
    send(app, NetworkScanMessage::TargetChanged(target.to_owned()));
    let effects = send(app, NetworkScanMessage::Start);
    match effects.as_slice() {
        [Effect::ScanNetwork { scan, .. }] => *scan,
        _ => panic!("no sweep started: {}", effects.len()),
    }
}

fn told(app: &mut App, scan: u64, host: ScannedHost) {
    send(
        app,
        NetworkScanMessage::Event {
            scan,
            event: ScanEvent::Host(host),
        },
    );
}

/// A saved profile of `protocol` at `host`:`port`.
fn seed(dir: &Path, protocol: DraftProtocol, host: &str, port: u16) {
    let mut draft = ProfileDraft::new_for(protocol);
    draft.set(ProfileField::Name, format!("saved {host}"));
    draft.set(ProfileField::Host, host.to_owned());
    draft.set(ProfileField::Port, port.to_string());
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    match draft.to_saved(ProfileId::new("seeded")).expect("profile") {
        DraftProfile::Ssh(profile) => store.merge([profile]),
        other => panic!("not an SSH profile: {other:?}"),
    };
    store.save().expect("save");
}

#[test]
fn the_dialog_opens_with_the_csharp_default_and_a_bad_target_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    send(&mut app, NetworkScanMessage::Open);
    assert_eq!(scan(&app).target, "192.168.1.0/24");

    send(
        &mut app,
        NetworkScanMessage::TargetChanged("example.com".into()),
    );
    assert!(send(&mut app, NetworkScanMessage::Start).is_empty());
    assert_eq!(scan(&app).error, Some(TargetError::Invalid));
    assert!(!scan(&app).running);

    send(
        &mut app,
        NetworkScanMessage::TargetChanged("10.0.0.0/8".into()),
    );
    assert!(send(&mut app, NetworkScanMessage::Start).is_empty());
    assert_eq!(scan(&app).error, Some(TargetError::PrefixOutOfRange));
}

#[test]
fn a_sweep_lists_hosts_with_their_suggested_session_and_closing_stops_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let run = start(&mut app, "10.0.0.0/24");
    assert!(scan(&app).running);
    assert_eq!(scan(&app).progress, (0, 254));

    send(
        &mut app,
        NetworkScanMessage::Event {
            scan: run,
            event: ScanEvent::Progress {
                done: 10,
                total: 254,
            },
        },
    );
    assert_eq!(scan(&app).progress, (10, 254));
    told(&mut app, run, host(1, Some("gw.lan"), &[22, 443]));
    told(&mut app, run, host(2, None, &[]));
    // An event of another sweep is ignored.
    told(&mut app, run + 1, host(3, None, &[22]));

    let rows = &scan(&app).rows;
    assert_eq!(rows.len(), 2);
    assert!(rows[0].choosable() && rows[0].chosen);
    assert!(
        !rows[1].choosable() && !rows[1].chosen,
        "no remote access port"
    );

    send(&mut app, NetworkScanMessage::Ended(run));
    assert!(!scan(&app).running && scan(&app).swept);
    // A second sweep cannot start while one is going, a new one can once it ended.
    assert_eq!(
        send(&mut app, NetworkScanMessage::Start).len(),
        1,
        "restarted once ended"
    );
    app.update(Message::DismissDialog);
    assert!(app.dialog.is_none());
    // The sweep closed with the dialog tells nothing more.
    told(&mut app, run + 1, host(4, None, &[22]));
    assert!(app.dialog.is_none());
}

#[test]
fn a_host_already_saved_for_the_same_session_is_not_offered_again() {
    let dir = tempfile::tempdir().expect("dir");
    seed(dir.path(), DraftProtocol::Ssh, "10.0.0.5", 22);
    seed_name(dir.path());
    let mut app = app(dir.path());
    let run = start(&mut app, "10.0.0.0/24");
    // Same address and kind: a duplicate; same address, another kind: not one.
    told(&mut app, run, host(5, None, &[22]));
    told(&mut app, run, host(5, None, &[3389]));
    // Found by the name a saved session has.
    told(&mut app, run, host(6, Some("Files.LAN"), &[22]));
    let rows = &scan(&app).rows;
    assert!(rows[0].duplicate && !rows[0].choosable() && !rows[0].chosen);
    assert!(!rows[1].duplicate && rows[1].choosable());
    assert!(rows[2].duplicate);
    assert_eq!(scan(&app).chosen(), 1);
}

/// A second saved session, reached by name.
fn seed_name(dir: &Path) {
    let mut draft = ProfileDraft::new_for(DraftProtocol::Ssh);
    draft.set(ProfileField::Name, "files".to_owned());
    draft.set(ProfileField::Host, "files.lan".to_owned());
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    match draft.to_saved(ProfileId::new("by-name")).expect("profile") {
        DraftProfile::Ssh(profile) => store.merge([profile]),
        other => panic!("not an SSH profile: {other:?}"),
    };
    store.save().expect("save");
}

#[test]
fn the_hosts_chosen_are_added_as_sessions_of_their_kind_without_credentials() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let run = start(&mut app, "10.0.0.0/24");
    told(&mut app, run, host(1, Some("dc01.lan"), &[3389, 22, 80]));
    told(&mut app, run, host(2, None, &[22]));
    told(&mut app, run, host(3, None, &[5900]));
    told(&mut app, run, host(4, None, &[80]));
    told(&mut app, run, host(5, None, &[443]));
    told(&mut app, run, host(6, None, &[5985]));
    told(&mut app, run, host(7, None, &[23]));
    told(&mut app, run, host(8, None, &[]));
    send(&mut app, NetworkScanMessage::Ended(run));
    assert_eq!(scan(&app).chosen(), 7);
    // The SSH host is left out by the user.
    send(&mut app, NetworkScanMessage::Choose(1));
    // A host that cannot be chosen stays unchosen.
    send(&mut app, NetworkScanMessage::Choose(7));
    assert_eq!(scan(&app).chosen(), 6);

    send(&mut app, NetworkScanMessage::Add);
    match &app.dialog {
        Some(Dialog::ScanAdded(summary)) => assert_eq!(summary.merged.added, 6),
        other => panic!("no report: {other:?}"),
    }

    let store = ProfileStore::open(dir.path().join("profiles.toml")).expect("saved");
    let rdp = store.rdp_profiles();
    assert_eq!(rdp.len(), 1);
    assert_eq!(rdp[0].name, "dc01.lan");
    assert_eq!(rdp[0].host, "10.0.0.1");
    assert_eq!(rdp[0].port, 3389);
    assert_eq!(rdp[0].group.as_deref(), Some("Discovered"));
    assert_eq!(rdp[0].username, None);
    assert_eq!(store.ssh_profiles().len(), 0);
    let vnc = store.vnc_profiles();
    assert_eq!((vnc.len(), vnc[0].port), (1, 5900));
    let web = store.web_profiles();
    assert_eq!(web.len(), 2);
    let plain = web.iter().find(|p| p.host == "10.0.0.4").expect("http");
    assert!(plain.use_http && plain.port == 80);
    let secure = web.iter().find(|p| p.host == "10.0.0.5").expect("https");
    assert!(!secure.use_http && secure.port == 443);
    let winrm = store.winrm_profiles();
    assert_eq!(
        (winrm.len(), winrm[0].port, winrm[0].use_ssl),
        (1, 5985, false)
    );
    let telnet = store.telnet_profiles();
    assert_eq!((telnet.len(), telnet[0].port), (1, 23));
}

#[test]
fn adding_with_nothing_chosen_does_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let run = start(&mut app, "10.0.0.1");
    told(&mut app, run, host(1, None, &[22]));
    send(&mut app, NetworkScanMessage::ChooseAll(false));
    assert_eq!(scan(&app).chosen(), 0);
    send(&mut app, NetworkScanMessage::Add);
    assert!(matches!(app.dialog, Some(Dialog::NetworkScan(_))));
    send(&mut app, NetworkScanMessage::ChooseAll(true));
    assert_eq!(scan(&app).chosen(), 1);
}
