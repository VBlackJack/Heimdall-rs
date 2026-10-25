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

use std::fs;
use std::path::PathBuf;

use heimdall_core::gateway_parents::MAX_GATEWAY_CHAIN_DEPTH;
use heimdall_core::last_good_copy::backup_path;
use heimdall_core::paths::PROFILES_FILE_NAME;
use heimdall_core::profile::{
    Forwards, LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId, RdpProfile,
    SshGateway, SshProfile, TelnetProfile, VncProfile, WinRmProfile,
};
use heimdall_core::store::{
    MergeReport, PROFILE_FILE_VERSION, ProfileStore, RouteError, StoreError, replace_file,
    replace_user_file,
};

fn profile(id: &str, host: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: None,
        host: host.to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: Some(PathBuf::from("/keys/admin")),
        gateway: None,
        local_tunnel_port: None,
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
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

#[test]
fn a_missing_file_is_an_empty_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    assert!(store.ssh_profiles().is_empty());
}

#[test]
fn saved_profiles_read_back_identically() {
    let dir = tempfile::tempdir().expect("temp dir");
    // A directory that does not exist yet: save must create it.
    let path = dir.path().join("nested").join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "h1"), profile("b", "h2")]);
    store.save().expect("saves");

    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.ssh_profiles(), store.ssh_profiles());
}

#[test]
fn save_leaves_no_temporary_file_behind() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "h1")]);
    store.save().expect("saves once");
    store.save().expect("saves over the existing file");
    let mut names: Vec<_> = fs::read_dir(dir.path())
        .expect("readable")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    names.sort();
    // The file and its last good copy, as the C# keeps one: nothing else.
    let copy = backup_path(&path);
    assert_eq!(
        names,
        vec![
            PROFILES_FILE_NAME.into(),
            copy.file_name().expect("named").to_owned()
        ]
    );
}

#[test]
fn a_store_saves_over_its_own_writes_again_and_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store
        .apply(|store| store.merge([profile("a", "h1")]))
        .expect("first save");
    store
        .apply(|store| store.merge([profile("b", "h2")]))
        .expect("saves over what it wrote");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    reopened
        .apply(|store| store.merge([profile("c", "h3")]))
        .expect("saves over what it read");
    assert_eq!(
        ProfileStore::open(&path)
            .expect("reopens")
            .ssh_profiles()
            .len(),
        3
    );
}

#[test]
fn a_file_changed_outside_is_not_overwritten() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut first = ProfileStore::open(&path).expect("opens");
    first
        .apply(|store| store.merge([profile("a", "h1")]))
        .expect("saves");
    // Another instance on the same file records a profile of its own.
    let mut other = ProfileStore::open(&path).expect("opens");
    other
        .apply(|store| store.merge([profile("b", "h2")]))
        .expect("saves");
    let written = fs::read_to_string(&path).expect("readable");

    let refused = first.apply(|store| store.merge([profile("c", "h3")]));

    assert!(
        matches!(refused, Err(StoreError::ChangedOutside { .. })),
        "{refused:?}"
    );
    assert_eq!(fs::read_to_string(&path).expect("readable"), written);
    let ids: Vec<_> = first.ssh_profiles().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["a"], "the store is left as it was");
}

#[test]
fn a_file_created_since_it_was_found_missing_is_not_overwritten() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut first = ProfileStore::open(&path).expect("opens");
    let mut other = ProfileStore::open(&path).expect("opens");
    other
        .apply(|store| store.merge([profile("b", "h2")]))
        .expect("saves");

    let refused = first.apply(|store| store.merge([profile("a", "h1")]));

    assert!(
        matches!(refused, Err(StoreError::ChangedOutside { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_file_removed_outside_is_written_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store
        .apply(|store| store.merge([profile("a", "h1")]))
        .expect("saves");
    fs::remove_file(&path).expect("removed");
    store
        .apply(|store| store.merge([profile("b", "h2")]))
        .expect("nothing there is lost");
    assert_eq!(
        ProfileStore::open(&path)
            .expect("reopens")
            .ssh_profiles()
            .len(),
        2
    );
}

#[test]
fn a_store_made_without_reading_replaces_what_is_there() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, format!("version = {PROFILE_FILE_VERSION}\n")).expect("writes");
    let mut store = ProfileStore::empty(&path);
    store
        .apply(|store| store.merge([profile("a", "h1")]))
        .expect("saves, as a store saving beside an unreadable file always has");
}

#[test]
fn merging_twice_updates_instead_of_duplicating() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");

    let first = store.merge([profile("a", "h1"), profile("b", "h2")]);
    assert_eq!(
        first,
        MergeReport {
            added: 2,
            updated: 0,
            unchanged: 0
        }
    );

    let second = store.merge([
        profile("a", "h1"),
        profile("b", "changed"),
        profile("c", "h3"),
    ]);
    assert_eq!(
        second,
        MergeReport {
            added: 1,
            updated: 1,
            unchanged: 1
        }
    );

    let hosts: Vec<_> = store
        .ssh_profiles()
        .iter()
        .map(|p| p.host.as_str())
        .collect();
    assert_eq!(hosts, vec!["h1", "changed", "h3"]);
}

#[test]
fn a_file_of_another_version_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, format!("version = {}\n", PROFILE_FILE_VERSION + 1)).expect("writes");
    assert!(matches!(
        ProfileStore::open(&path),
        Err(StoreError::UnsupportedVersion { .. })
    ));
}

#[test]
fn a_corrupt_file_is_an_error_not_an_empty_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, "version = [").expect("writes");
    assert!(matches!(
        ProfileStore::open(&path),
        Err(StoreError::Parse { .. })
    ));
}

#[test]
fn a_file_left_unreadable_is_read_from_its_last_good_copy_then_replaced_at_the_next_save() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store
        .apply(|store| store.merge([profile("web", "web.lab")]))
        .expect("first save");
    assert!(!backup_path(&path).exists(), "nothing replaced yet");
    store
        .apply(|store| store.merge([profile("db", "db.lab")]))
        .expect("second save");
    assert!(
        backup_path(&path).exists(),
        "the first file kept, as the C# .bak"
    );
    // As a power cut mid-save leaves it.
    fs::write(&path, vec![0_u8; 64]).expect("zeroed");
    let mut reopened = ProfileStore::open(&path).expect("read from the copy");
    assert!(reopened.recovered_from_backup());
    let ids: Vec<&str> = reopened
        .ssh_profiles()
        .iter()
        .map(|profile| profile.id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["web"],
        "the last good copy, not the profile saved after it"
    );
    reopened
        .apply(|store| store.merge([profile("app", "app.lab")]))
        .expect("the damaged file is replaced");
    let again = ProfileStore::open(&path).expect("reads");
    assert!(!again.recovered_from_backup());
    assert_eq!(again.ssh_profiles().len(), 2);
    let copy = ProfileStore::open(backup_path(&path)).expect("the copy reads");
    assert_eq!(
        copy.ssh_profiles().len(),
        1,
        "the zeroed file never replaced the good copy"
    );
}

#[test]
fn an_unreadable_file_without_a_usable_copy_fails_as_before() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, "").expect("emptied");
    fs::write(backup_path(&path), "version = [").expect("broken copy");
    assert!(matches!(
        ProfileStore::open(&path),
        Err(StoreError::Parse { .. })
    ));
}

fn rdp(id: &str) -> RdpProfile {
    RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: Some("LAB".to_owned()),
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
    }
}

#[test]
fn a_version_1_file_still_opens_and_is_saved_as_the_current_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 1\n\n[[ssh]]\nid = \"a\"\nname = \"A\"\nhost = \"h\"\nport = 22\n",
    )
    .expect("writes");
    let mut store = ProfileStore::open(&path).expect("a version 1 file opens");
    assert_eq!(store.ssh_profiles().len(), 1);
    assert!(store.rdp_profiles().is_empty());
    store.merge_rdp([rdp("r")]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(
        text.starts_with(&format!("version = {PROFILE_FILE_VERSION}\n")),
        "{text}"
    );
    assert_eq!(PROFILE_FILE_VERSION, 16);
}

#[test]
fn rdp_profiles_read_back_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    let report = store.merge_rdp([rdp("r")]);
    assert_eq!(report.added, 1);
    store.save().expect("saves");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.rdp_profiles(), store.rdp_profiles());
    assert!(reopened.remove(&ProfileId::new("r")));
    assert!(reopened.rdp_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
    assert!(!reopened.remove(&ProfileId::new("r")), "already gone");
}

fn telnet(id: &str) -> TelnetProfile {
    TelnetProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: Some("Network".to_owned()),
        host: "switch.lab".to_owned(),
        port: 23,
        session_logging: None,
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

#[test]
fn a_version_2_file_with_rdp_profiles_still_opens() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 2\n\n[[rdp]]\nid = \"r\"\nname = \"R\"\nhost = \"h\"\nport = 3389\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("a version 2 file opens");
    assert_eq!(store.rdp_profiles().len(), 1);
    assert!(store.telnet_profiles().is_empty());
}

#[test]
fn telnet_profiles_read_back_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    store.merge_rdp([rdp("r")]);
    let report = store.merge_telnet([telnet("t")]);
    assert_eq!(report.added, 1);
    assert_eq!(store.merge_telnet([telnet("t")]).unchanged, 1);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains("[[telnet]]"), "{text}");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.telnet_profiles(), store.telnet_profiles());
    assert!(reopened.remove(&ProfileId::new("t")));
    assert!(reopened.telnet_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
    assert_eq!(reopened.rdp_profiles().len(), 1, "the RDP profile stays");
    assert!(!reopened.remove(&ProfileId::new("t")), "already gone");
}

#[test]
fn vnc_profiles_read_back_with_their_options_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_telnet([telnet("t")]);
    let watch = VncProfile {
        id: ProfileId::new("v"),
        name: "Kiosk".to_owned(),
        group: None,
        host: "kiosk.lab".to_owned(),
        port: 5901,
        view_only: true,
        allow_no_password: false,
        require_tls: false,
        username: None,
        vault_entry: None,
    };
    assert_eq!(store.merge_vnc([watch.clone()]).added, 1);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(
        text.contains("[[vnc]]") && text.contains("view_only = true"),
        "{text}"
    );
    assert!(
        !text.contains("allow_no_password"),
        "a false option is left out: {text}"
    );
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.vnc_profiles(), [watch]);
    assert!(reopened.remove(&ProfileId::new("v")));
    assert!(reopened.vnc_profiles().is_empty());
    assert_eq!(
        reopened.telnet_profiles().len(),
        1,
        "the Telnet profile stays"
    );
}

fn local(id: &str, line: &str) -> LocalProfile {
    LocalProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: Some("Tools".to_owned()),
        command: LocalCommand {
            program: Some("pwsh.exe".to_owned()),
            arguments: LocalArguments::WindowsLine(line.to_owned()),
            working_directory: Some(PathBuf::from(r"C:\work")),
            run_as_administrator: false,
        },
        approved: None,
        session_logging: None,
    }
}

fn approval_of(profile: &LocalProfile) -> LocalApproval {
    LocalApproval {
        command: profile.command.clone(),
        program_path: PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe"),
    }
}

#[test]
fn local_profiles_read_back_with_their_approval_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    let tool = local("l", "-NoExit -Command \"ssh a\"");
    assert_eq!(store.merge_local([tool.clone()]).added, 1);
    assert!(store.approve_local(&ProfileId::new("l"), approval_of(&tool)));
    assert!(!store.approve_local(&ProfileId::new("nobody"), approval_of(&tool)));
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains("[[local]]"), "{text}");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.local_profiles(), store.local_profiles());
    assert_eq!(
        reopened.local_profiles()[0].approved,
        Some(approval_of(&tool))
    );
    assert!(reopened.remove(&ProfileId::new("l")));
    assert!(reopened.local_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
}

#[test]
fn a_reimport_keeps_the_approval_and_never_brings_one() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    let tool = local("l", "-NoExit");
    store.merge_local([tool.clone()]);
    store.approve_local(&tool.id, approval_of(&tool));

    // The same profile again, as an import brings it: no approval of its own.
    let report = store.merge_local([tool.clone()]);
    assert_eq!(report.unchanged, 1, "{report:?}");
    assert_eq!(store.local_profiles()[0].approved, Some(approval_of(&tool)));

    // Changed: kept as the record of what was approved, which no longer matches.
    let changed = local("l", "-NoExit -Command calc");
    assert_eq!(store.merge_local([changed.clone()]).updated, 1);
    let stored = &store.local_profiles()[0];
    assert_eq!(stored.command, changed.command);
    assert_eq!(stored.approved, Some(approval_of(&tool)));
    assert!(!stored.may_run(&approval_of(&tool).program_path));

    // An incoming approval is dropped, whatever it says.
    let mut forged = local("m", "-Command calc");
    forged.approved = Some(approval_of(&forged));
    store.merge_local([forged]);
    assert_eq!(store.local_profiles()[1].approved, None);
}

fn gateway(id: &str, parent: Option<&str>) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        host: format!("{id}.lab"),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: parent.map(ProfileId::new),
    }
}

#[test]
fn a_route_runs_from_the_farthest_parent_to_the_profile_s_gateway() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_gateways([
        gateway("inner", Some("middle")),
        gateway("outer", None),
        gateway("middle", Some("outer")),
    ]);
    store.save().expect("saves");
    let store = ProfileStore::open(&path).expect("reopens");
    assert_eq!(store.gateways().len(), 3);
    let route: Vec<String> = store
        .route(Some(&ProfileId::new("inner")))
        .expect("route")
        .iter()
        .map(|gateway| gateway.id.to_string())
        .collect();
    assert_eq!(
        route,
        ["outer", "middle", "inner"],
        "nearest to this machine first"
    );
    assert!(store.route(None).expect("no gateway").is_empty());
}

#[test]
fn a_route_through_a_missing_gateway_or_a_loop_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    store.merge_gateways([
        gateway("orphan", Some("gone")),
        gateway("one", Some("two")),
        gateway("two", Some("one")),
    ]);
    assert_eq!(
        store.route(Some(&ProfileId::new("orphan"))),
        Err(RouteError::MissingGateway(ProfileId::new("gone")))
    );
    assert_eq!(
        store.route(Some(&ProfileId::new("nobody"))),
        Err(RouteError::MissingGateway(ProfileId::new("nobody")))
    );
    assert!(matches!(
        store.route(Some(&ProfileId::new("one"))),
        Err(RouteError::Loop(_))
    ));
}

#[test]
fn a_route_deeper_than_five_gateways_is_refused_as_the_csharp_does() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    // g1 <- g2 <- ... <- g6.
    let mut chain = vec![gateway("g1", None)];
    for level in 2..=MAX_GATEWAY_CHAIN_DEPTH + 1 {
        chain.push(gateway(
            &format!("g{level}"),
            Some(&format!("g{}", level - 1)),
        ));
    }
    store.merge_gateways(chain);
    let deepest_allowed = ProfileId::new(format!("g{MAX_GATEWAY_CHAIN_DEPTH}"));
    assert_eq!(
        store.route(Some(&deepest_allowed)).expect("five").len(),
        MAX_GATEWAY_CHAIN_DEPTH
    );
    let too_deep = ProfileId::new(format!("g{}", MAX_GATEWAY_CHAIN_DEPTH + 1));
    assert_eq!(
        store.route(Some(&too_deep)),
        Err(RouteError::TooDeep(ProfileId::new("g1"))),
        "the gateway one too many"
    );
}

#[test]
fn a_version_5_file_still_opens() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 5\n\n[[ssh]]\nid = \"a\"\nname = \"A\"\nhost = \"h\"\nport = 22\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("a version 5 file opens");
    assert_eq!(store.ssh_profiles()[0].gateway, None);
    assert!(store.gateways().is_empty());
}

#[test]
fn the_clipboard_setting_is_written_only_when_turned_off() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    let mut off = rdp("off");
    off.redirect_clipboard = false;
    store.merge_rdp([rdp("on"), off]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert_eq!(text.matches("redirect_clipboard").count(), 1, "{text}");
    let reopened = ProfileStore::open(&path).expect("reopens");
    let shared: Vec<bool> = reopened
        .rdp_profiles()
        .iter()
        .map(|profile| profile.redirect_clipboard)
        .collect();
    assert_eq!(shared, [true, false]);
}

#[test]
fn the_drives_setting_is_written_only_when_turned_on() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    let mut on = rdp("on");
    on.redirect_drives = true;
    store.merge_rdp([rdp("off"), on]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert_eq!(text.matches("redirect_drives").count(), 1, "{text}");
    let reopened = ProfileStore::open(&path).expect("reopens");
    let shared: Vec<bool> = reopened
        .rdp_profiles()
        .iter()
        .map(|profile| profile.redirect_drives)
        .collect();
    assert_eq!(shared, [false, true]);
}

#[test]
fn the_display_and_session_options_are_written_only_when_not_the_defaults() {
    use heimdall_core::profile::{AudioPlayback, ColorDepth, RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    let mut chosen = rdp("chosen");
    chosen.options = RdpOptions {
        aspect: heimdall_core::profile::Aspect::Stretch,
        color_depth: ColorDepth::Bpp16,
        audio: AudioPlayback::OnServer,
        admin_session: true,
        resolution: Resolution::Fixed,
        fixed_width: 1280,
        fixed_height: 720,
        scale_fixed: false,
        dynamic_resolution: false,
        performance_flags: 0,
        resize_enable_delay_ms: None,
    };
    store.merge_rdp([rdp("defaults"), chosen.clone()]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    for line in [
        "color_depth = 16",
        "audio = \"on-server\"",
        "admin_session = true",
        "resolution = \"fixed\"",
        "fixed_width = 1280",
        "fixed_height = 720",
        "scale_fixed = false",
        "dynamic_resolution = false",
    ] {
        let key = format!(
            "
{}",
            &line[..=line.find(' ').expect("a key")]
        );
        assert_eq!(text.matches(&key).count(), 1, "{key}: {text}");
        assert!(text.contains(line), "{line}: {text}");
    }
    let reopened = ProfileStore::open(&path).expect("reopens");
    let options: Vec<RdpOptions> = reopened
        .rdp_profiles()
        .iter()
        .map(|profile| profile.options)
        .collect();
    assert_eq!(options, [RdpOptions::default(), chosen.options]);
    assert_eq!(
        RdpOptions::default(),
        RdpOptions {
            aspect: heimdall_core::profile::Aspect::Stretch,
            color_depth: ColorDepth::Bpp32,
            audio: AudioPlayback::Off,
            admin_session: false,
            resolution: Resolution::FitWindow,
            fixed_width: 1920,
            fixed_height: 1080,
            scale_fixed: true,
            dynamic_resolution: true,
            performance_flags: 0,
            resize_enable_delay_ms: None,
        },
        "the C# defaults"
    );
}

#[test]
fn a_colour_depth_the_session_cannot_have_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_rdp([rdp("one")]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    let edited = text.replace("[[rdp]]\n", "[[rdp]]\ncolor_depth = 8\n");
    assert_ne!(edited, text, "the depth landed");
    fs::write(&path, edited).expect("writes");
    let error = ProfileStore::open(&path).expect_err("refused").to_string();
    assert!(error.contains("colour depth 8"), "{error}");
}

fn winrm(id: &str) -> WinRmProfile {
    WinRmProfile {
        id: ProfileId::new(id),
        name: format!("WinRM {id}"),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 5986,
        use_ssl: true,
        skip_certificate_check: true,
        username: Some("LAB\\admin".to_owned()),
        gateway: None,
        local_tunnel_port: None,
    }
}

#[test]
fn winrm_profiles_read_back_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    let moved = WinRmProfile {
        port: 5985,
        use_ssl: false,
        ..winrm("w")
    };
    let report = store.merge_winrm([moved, winrm("w")]);
    assert_eq!(report.added, 1);
    assert_eq!(report.updated, 1);
    store.save().expect("saves");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.winrm_profiles(), [winrm("w")]);
    assert!(reopened.remove(&ProfileId::new("w")));
    assert!(reopened.winrm_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
}

#[test]
fn a_version_6_file_opens_without_winrm_profiles() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 6\n\n[[ssh]]\nid = \"a\"\nname = \"A\"\nhost = \"h\"\nport = 22\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("a version 6 file opens");
    assert_eq!(store.ssh_profiles().len(), 1);
    assert!(store.winrm_profiles().is_empty());
}

#[test]
fn a_vault_entry_name_is_written_only_when_set_and_read_back() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([
        SshProfile {
            vault_entry: Some("Servers/Web".to_owned()),
            ..profile("a", "h1")
        },
        profile("b", "h2"),
    ]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert_eq!(text.matches("vault_entry").count(), 1, "{text}");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(
        reopened.ssh_profiles()[0].vault_entry.as_deref(),
        Some("Servers/Web")
    );
    assert_eq!(reopened.ssh_profiles()[1].vault_entry, None);
}

#[test]
fn a_renamed_profile_keeps_its_old_name_as_its_vault_entry_as_the_csharp_rename_freezes_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    store.merge([profile("a", "h1")]);
    store.merge_rdp([rdp("r")]);
    store.merge_vnc([VncProfile {
        id: ProfileId::new("v"),
        name: "Kiosk".to_owned(),
        group: None,
        host: "kiosk.lab".to_owned(),
        port: 5901,
        view_only: false,
        allow_no_password: false,
        require_tls: false,
        username: None,
        vault_entry: None,
    }]);
    store.merge_telnet([telnet("t")]);
    for id in ["a", "r", "v", "t"] {
        assert!(store.rename_profile(&ProfileId::new(id), "  Renamed  "));
    }
    assert_eq!(store.ssh_profiles()[0].name, "Renamed");
    assert_eq!(store.ssh_profiles()[0].vault_entry.as_deref(), Some("A"));
    assert_eq!(store.rdp_profiles()[0].vault_entry.as_deref(), Some("R"));
    assert_eq!(
        store.vnc_profiles()[0].vault_entry.as_deref(),
        Some("Kiosk")
    );
    assert_eq!(
        store.telnet_profiles()[0].name,
        "Renamed",
        "renamed all the same"
    );

    // Renamed again, the entry set stays; the same name freezes nothing.
    assert!(store.rename_profile(&ProfileId::new("a"), "Again"));
    assert_eq!(store.ssh_profiles()[0].vault_entry.as_deref(), Some("A"));
    let mut fresh = ProfileStore::open(dir.path().join("other.toml")).expect("opens");
    fresh.merge([profile("b", "h2")]);
    assert!(fresh.rename_profile(&ProfileId::new("b"), "B"));
    assert_eq!(fresh.ssh_profiles()[0].vault_entry, None, "not renamed");
    assert!(!fresh.rename_profile(&ProfileId::new("b"), "  "));
    assert!(!fresh.rename_profile(&ProfileId::new("none"), "X"));
}

#[test]
fn a_socks_port_is_saved_only_when_set() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("none", "h1")]);
    store.save().expect("saves");
    assert!(
        !fs::read_to_string(&path)
            .expect("read")
            .contains("socks_port"),
        "no proxy, nothing written"
    );

    let mut proxied = profile("proxied", "h2");
    proxied.forwards = Forwards {
        socks_port: Some(1080),
        ..Forwards::default()
    };
    store.merge([proxied]);
    store.save().expect("saves");
    let reopened = ProfileStore::open(&path).expect("reopens");
    let ports: Vec<_> = reopened
        .ssh_profiles()
        .iter()
        .map(|profile| (profile.id.as_str(), profile.forwards.socks_port))
        .collect();
    assert_eq!(ports, [("none", None), ("proxied", Some(1080))]);
}

#[test]
fn the_store_keeps_an_approval_of_post_connect_steps_and_never_takes_one_from_an_import() {
    use heimdall_core::post_connect::{PostConnect, PostConnectStep};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let steps = vec![PostConnectStep::new("sudo -i")];
    let with = |approved: Option<Vec<PostConnectStep>>| {
        let mut profile = profile("a", "h1");
        profile.post_connect = PostConnect {
            steps: steps.clone(),
            approved,
        };
        profile
    };
    let mut store = ProfileStore::open(&path).expect("opens");
    // An approval arriving with the profile is not taken.
    store.merge([with(Some(steps.clone()))]);
    assert!(!store.ssh_profiles()[0].post_connect.is_approved());
    // Approving other steps than those saved approves nothing saved.
    assert!(store.approve_post_connect(&ProfileId::new("a"), &[PostConnectStep::new("ls")]));
    assert!(store.ssh_profiles()[0].post_connect.needs_approval());
    assert!(store.approve_post_connect(&ProfileId::new("a"), &steps));
    assert!(!store.approve_post_connect(&ProfileId::new("gone"), &steps));
    store.save().expect("saves");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert!(reopened.ssh_profiles()[0].post_connect.is_approved());

    // The same steps merged again keep the approval; other steps are not approved by it.
    store.merge([with(None)]);
    assert!(store.ssh_profiles()[0].post_connect.is_approved());
    let mut changed = with(None);
    changed.post_connect.steps = vec![PostConnectStep::new("rm -rf /")];
    store.merge([changed]);
    assert!(store.ssh_profiles()[0].post_connect.needs_approval());
}

#[test]
fn ftp_profiles_read_back_and_passive_is_written_only_when_off() {
    use heimdall_core::profile::FtpProfile;

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let ftp = |id: &str, passive: bool| FtpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: Some("Files".to_owned()),
        host: "ftp.lab".to_owned(),
        port: 21,
        username: Some("ops".to_owned()),
        passive,
        tls: true,
        vault_entry: None,
    };
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_ftp([ftp("passive", true)]);
    store.save().expect("saves");
    assert!(
        !fs::read_to_string(&path)
            .expect("read")
            .contains("passive = "),
        "the default is not written"
    );
    store.merge_ftp([ftp("active", false)]);
    store.save().expect("saves");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(
        reopened.ftp_profiles(),
        [ftp("passive", true), ftp("active", false)]
    );
    let mut store = reopened;
    assert!(store.remove(&ProfileId::new("active")));
    assert_eq!(store.ftp_profiles().len(), 1);
}

#[test]
fn citrix_profiles_read_back_with_their_launch_settings() {
    use heimdall_core::profile::CitrixProfile;

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let store_front = CitrixProfile {
        id: ProfileId::new("outlook"),
        name: "Outlook".to_owned(),
        group: Some("Apps".to_owned()),
        store_front_url: Some("https://store.lab/Citrix/Store".to_owned()),
        app_name: Some("Outlook 365".to_owned()),
        ica_file: None,
        seamless: true,
        sso: true,
        citrix_id: None,
    };
    let ica = CitrixProfile {
        id: ProfileId::new("erp"),
        name: "ERP".to_owned(),
        group: None,
        store_front_url: None,
        app_name: None,
        ica_file: Some(r"C:\apps\erp.ica".to_owned()),
        seamless: false,
        sso: false,
        citrix_id: None,
    };
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_citrix([store_front.clone(), ica.clone()]);
    store.save().expect("saves");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.citrix_profiles(), [store_front, ica]);
    let mut store = reopened;
    assert!(store.remove(&ProfileId::new("erp")));
    assert_eq!(store.citrix_profiles().len(), 1);
}

fn web_console(id: &str) -> heimdall_core::profile::WebProfile {
    heimdall_core::profile::WebProfile {
        id: ProfileId::new(id),
        name: "iLO".to_owned(),
        group: Some("Lab/BMC".to_owned()),
        host: "ilo.lab".to_owned(),
        port: 8443,
        use_http: false,
        path: Some("/html/login.html".to_owned()),
        username: Some("admin".to_owned()),
        gateway: Some(ProfileId::new("bastion")),
    }
}

#[test]
fn web_consoles_read_back_with_their_address_and_gateway() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let routed = web_console("ilo");
    let plain = heimdall_core::profile::WebProfile {
        id: ProfileId::new("nas"),
        name: "NAS".to_owned(),
        group: None,
        host: "fd00::20".to_owned(),
        port: 80,
        use_http: true,
        path: None,
        username: None,
        gateway: None,
    };
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_web([routed.clone(), plain.clone()]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains("[[web]]"), "{text}");
    assert!(!text.to_lowercase().contains("password"), "{text}");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.web_profiles(), [routed, plain]);
    assert!(
        reopened.folder_paths().contains(&"Lab/BMC".to_owned()),
        "its folder is a folder of the tree"
    );
    let mut store = reopened;
    assert!(store.set_port(&ProfileId::new("nas"), 8080));
    assert!(
        !store.set_gateway(&ProfileId::new("nas"), None),
        "direct already"
    );
    assert!(store.rename_profile(&ProfileId::new("nas"), "Storage"));
    assert!(store.remove(&ProfileId::new("ilo")));
    assert_eq!(store.web_profiles().len(), 1);
    assert_eq!(store.web_profiles()[0].port, 8080);
    assert_eq!(store.web_profiles()[0].name, "Storage");
}

#[test]
fn tool_entries_read_back_and_follow_the_profile_operations() {
    use heimdall_core::profile::ToolProfile;
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let ping = ToolProfile {
        id: ProfileId::new("ping-core"),
        name: "Ping core".to_owned(),
        group: Some("Lab/Tools".to_owned()),
        tool: "PING".to_owned(),
        host: Some("core.lab".to_owned()),
        port: Some(443),
    };
    let uuid = ToolProfile {
        id: ProfileId::new("uuid"),
        name: "UUID".to_owned(),
        group: None,
        tool: "UUID".to_owned(),
        host: None,
        port: None,
    };
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_tools([ping.clone(), uuid.clone()]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains("[[tool]]"), "{text}");
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.tool_profiles(), [ping, uuid]);
    assert!(
        reopened.folder_paths().contains(&"Lab/Tools".to_owned()),
        "its folder is a folder of the tree"
    );
    let mut store = reopened;
    assert!(store.rename_profile(&ProfileId::new("uuid"), "Identifiers"));
    assert_eq!(store.tool_profiles()[1].name, "Identifiers");
    assert!(store.remove(&ProfileId::new("ping-core")));
    assert_eq!(store.tool_profiles().len(), 1);
}

#[test]
fn a_file_written_before_tool_entries_still_reads() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 13\n[[ssh]]\nid = \"web\"\nname = \"web\"\nhost = \"h\"\nport = 22\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("a version 13 file opens");
    assert_eq!(store.ssh_profiles().len(), 1);
    assert!(store.tool_profiles().is_empty());
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(
        text.starts_with(&format!("version = {PROFILE_FILE_VERSION}\n")),
        "{text}"
    );
    assert!(
        !text.contains("[[tool]]"),
        "nothing written when none: {text}"
    );
}

#[test]
fn a_web_console_goes_direct_once_its_gateway_is_removed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    store.merge_web([web_console("ilo")]);
    let removed = store
        .remove_gateway(&ProfileId::new("bastion"))
        .expect("a gateway");
    assert_eq!(removed.servers, 1);
    assert_eq!(store.web_profiles()[0].gateway, None);
}

#[test]
fn a_version_12_file_opens_without_web_consoles_and_one_written_now_says_the_current_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 12\n\n[[citrix]]\nid = \"c\"\nname = \"C\"\n",
    )
    .expect("writes");
    let mut store = ProfileStore::open(&path).expect("a version 12 file opens");
    assert_eq!(store.citrix_profiles().len(), 1);
    assert!(store.web_profiles().is_empty());
    store.merge_web([web_console("ilo")]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(
        text.starts_with(&format!("version = {PROFILE_FILE_VERSION}\n")),
        "{text}"
    );
    // A web console's own switches are written only when set.
    assert!(!text.contains("use_http"), "{text}");
}

#[test]
fn favorites_are_kept_by_profile_and_leave_with_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "a.lab"), profile("b", "b.lab")]);
    let a = ProfileId::new("a");
    assert!(!store.is_favorite(&a));
    assert!(store.set_favorite(&a, true));
    assert!(!store.set_favorite(&a, true), "already one");
    assert!(
        !store.set_favorite(&ProfileId::new("gone"), true),
        "a profile not in the store is not marked"
    );
    store.save().expect("saves");
    let mut reread = ProfileStore::open(&path).expect("reopens");
    assert!(reread.is_favorite(&a));
    assert!(!reread.is_favorite(&ProfileId::new("b")));
    assert!(reread.remove(&a));
    assert!(!reread.is_favorite(&a), "deleted: a favorite no longer");
    assert!(reread.set_favorite(&ProfileId::new("b"), true));
    assert!(reread.set_favorite(&ProfileId::new("b"), false));
    assert!(!reread.is_favorite(&ProfileId::new("b")));
}

#[test]
fn a_port_and_an_account_are_set_where_a_profile_has_them() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    store.merge([profile("a", "a.lab")]);
    store.merge_telnet([telnet("t")]);
    let a = ProfileId::new("a");
    assert!(store.set_port(&a, 2200));
    assert!(!store.set_port(&a, 2200), "already so");
    assert_eq!(store.ssh_profiles()[0].port, 2200);
    assert!(store.set_username(&a, "ops"));
    assert!(!store.set_username(&a, "ops"));
    assert!(store.set_port(&ProfileId::new("t"), 2323));
    assert!(
        !store.set_username(&ProfileId::new("t"), "ops"),
        "Telnet names no account"
    );
    assert!(!store.set_port(&ProfileId::new("gone"), 1));
}

#[test]
fn a_profile_s_metadata_is_kept_beside_it_read_leniently_and_goes_with_it() {
    use heimdall_core::metadata::{Environment, ProfileMetadata};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("store");
    store.merge([profile("web", "web.lab")]);
    let id = store.ssh_profiles()[0].id.clone();
    let metadata = ProfileMetadata {
        origin: None,
        sort_order: None,
        tunnels_expanded: None,
        environment: Some(Environment::Staging),
        tags: "frontend".to_owned(),
        mac_address: "0A:0B:0C:0D:0E:0F".parse().ok(),
    };
    assert!(
        !store.set_metadata(
            &heimdall_core::profile::ProfileId::new("nobody"),
            metadata.clone()
        ),
        "a profile not in the store keeps none"
    );
    assert!(store.set_metadata(&id, metadata.clone()));
    store.save().expect("save");
    let mut store = ProfileStore::open(&path).expect("read back");
    assert_eq!(store.metadata(&id), Some(&metadata));
    assert!(store.remove(&id));
    assert_eq!(store.metadata(&id), None, "gone with it");

    // An environment or an address the file names that does not read is left out.
    fs::write(
        &path,
        format!(
            "version = {PROFILE_FILE_VERSION}\n[[ssh]]\nid = \"web\"\nname = \"web\"\nhost = \"h\"\nport = 22\n\n[metadata.web]\nenvironment = \"Moon\"\ntags = \"kept\"\nmac_address = \"zz\"\n"
        ),
    )
    .expect("write");
    let store = ProfileStore::open(&path).expect("read");
    let read = store
        .metadata(&heimdall_core::profile::ProfileId::new("web"))
        .expect("the tags");
    assert_eq!(
        (read.environment, read.tags.as_str(), read.mac_address),
        (None, "kept", None)
    );
}

#[test]
fn an_rdp_profile_keeps_what_the_built_in_client_does_not_use_and_its_metadata_its_origin() {
    use heimdall_core::metadata::{ProfileMetadata, ProfileOrigin};
    use heimdall_core::profile::{Aspect, RdpExtras, RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    let kept = RdpProfile {
        options: RdpOptions {
            resolution: Resolution::MultiMonitor,
            aspect: Aspect::UltraWide,
            ..RdpOptions::default()
        },
        extras: RdpExtras {
            rd_gateway: Some("rdg.lab".to_owned()),
            redirect_usb: true,
            monitors: vec![1],
            compression: false,
            hardware_acceleration: true,
            ..RdpExtras::default()
        },
        ..rdp("dc")
    };
    store.merge_rdp([kept.clone(), rdp("plain")]);
    let id = ProfileId::new("dc");
    store.set_metadata(
        &id,
        ProfileMetadata {
            tags: "domain".to_owned(),
            sort_order: Some(-2),
            tunnels_expanded: Some(true),
            ..ProfileMetadata::default()
        },
    );
    assert!(store.set_origin(&id, ProfileOrigin::RdcMan));
    store.save().expect("saves");

    let text = fs::read_to_string(&path).expect("read");
    assert!(
        !text.contains("bitmap_caching"),
        "a default is not written down:\n{text}"
    );
    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.rdp_profiles(), [kept, rdp("plain")]);
    assert_eq!(
        reopened.metadata(&id),
        Some(&ProfileMetadata {
            tags: "domain".to_owned(),
            origin: Some(ProfileOrigin::RdcMan),
            sort_order: Some(-2),
            tunnels_expanded: Some(true),
            ..ProfileMetadata::default()
        }),
        "the origin marked, the tags kept"
    );
}

#[test]
fn each_origin_has_the_csharp_badge_code() {
    use heimdall_core::metadata::ProfileOrigin;
    let codes: Vec<&str> = ProfileOrigin::ALL
        .into_iter()
        .map(ProfileOrigin::badge)
        .collect();
    assert_eq!(codes, ["RDP", "OSSH", "PTY", "MRNG", "MXTM", "RDCM"]);
}

#[test]
fn every_ssh_shell_takes_a_mode_and_sftp_profiles_keep_theirs() {
    use heimdall_core::profile::SshMode;

    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    let mut external = profile("b", "b.lab");
    external.ssh_mode = SshMode::External;
    let mut sftp = profile("c", "c.lab");
    sftp.sftp = true;
    store.merge([profile("a", "a.lab"), external, sftp]);
    assert_eq!(store.ssh_mode_changes(SshMode::External), (1, 2));
    assert_eq!(store.ssh_mode_changes(SshMode::Embedded), (1, 2));
    assert_eq!(store.set_ssh_modes(SshMode::External), 1);
    let modes: Vec<SshMode> = store
        .ssh_profiles()
        .iter()
        .map(|profile| profile.ssh_mode)
        .collect();
    assert_eq!(
        modes,
        [SshMode::External, SshMode::External, SshMode::Embedded]
    );
    assert_eq!(store.set_ssh_modes(SshMode::External), 0, "already so");
    assert_eq!(store.ssh_mode_changes(SshMode::External), (0, 2));
}

#[test]
fn every_rdp_profile_takes_a_mode() {
    use heimdall_core::profile::RdpMode;

    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    let set = |id: &str, external: bool| {
        let mut profile = rdp(id);
        profile.extras.external = external;
        profile
    };
    store.merge_rdp([set("a", false), set("b", true), set("c", false)]);
    // An SSH profile is not counted.
    store.merge([profile("s", "s.lab")]);
    assert_eq!(store.rdp_mode_changes(RdpMode::External), (2, 3));
    assert_eq!(store.rdp_mode_changes(RdpMode::Embedded), (1, 3));
    assert_eq!(store.set_rdp_modes(RdpMode::External), 2);
    let modes: Vec<bool> = store
        .rdp_profiles()
        .iter()
        .map(|profile| profile.extras.external)
        .collect();
    assert_eq!(modes, [true, true, true]);
    assert_eq!(store.set_rdp_modes(RdpMode::External), 0, "already so");
    assert_eq!(store.rdp_mode_changes(RdpMode::External), (0, 3));
    assert!(
        store.ssh_profiles()[0].ssh_mode.is_embedded(),
        "SSH untouched"
    );
}

/// The files of `dir`, by name.
fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("listed")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_file_replaced_is_the_new_text_whole_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("sub").join("d.mmd");
    replace_file(
        &path,
        b"graph TD
",
    )
    .expect("made with its folder");
    replace_user_file(&path, b"x").expect("replaced");
    assert_eq!(fs::read(&path).expect("read"), b"x");
    assert_eq!(names_in(&dir.path().join("sub")), ["d.mmd"]);
}

#[test]
fn a_failed_replace_leaves_the_old_file_untouched() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("taken");
    // A folder where the file goes: the rename fails, after the text was written aside.
    fs::create_dir(&path).expect("folder");
    fs::write(path.join("kept"), "old").expect("written");
    assert!(replace_user_file(&path, b"new").is_err());
    assert_eq!(fs::read_to_string(path.join("kept")).expect("read"), "old");
    assert_eq!(names_in(dir.path()), ["taken"], "no temporary file left");
}

#[test]
fn a_read_only_user_file_is_refused_as_writing_over_it_would_be() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("d.mmd");
    fs::write(&path, "old").expect("written");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).expect("read-only");
    let error = replace_user_file(&path, b"new").expect_err("refused");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read_to_string(&path).expect("read"), "old");
    // Writable again, so the folder can be removed on every platform.
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "the test's own file, made writable to be removed"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).expect("writable");
}

#[cfg(unix)]
#[test]
fn a_user_file_keeps_its_mode_and_a_link_to_it_stays_a_link() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("d.mmd");
    fs::write(&path, "old").expect("written");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("mode");
    let link = dir.path().join("link.mmd");
    std::os::unix::fs::symlink(&path, &link).expect("link");
    replace_user_file(&link, b"new").expect("replaced");
    assert!(
        fs::symlink_metadata(&link)
            .expect("metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&path).expect("read"), "new");
    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
}

/// The `share_mode` flags of `CreateFileW`, by their documented values.
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x1;
#[cfg(windows)]
const FILE_SHARE_WRITE: u32 = 0x2;
#[cfg(windows)]
const FILE_SHARE_DELETE: u32 = 0x4;

/// `path` opened for reading as another program would, sharing it with `share_mode`.
#[cfg(windows)]
fn held_open(path: &std::path::Path, share_mode: u32) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt as _;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(share_mode)
        .open(path)
        .expect("held open")
}

#[cfg(windows)]
#[test]
fn a_file_another_program_holds_open_with_delete_sharing_is_replaced() {
    use std::io::Read as _;
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("profiles.json");
    fs::write(&path, "old").expect("written");
    // An editor, an antivirus or a sync client reading the file: a bare MoveFileExW is
    // refused then, the POSIX rename replaces it (on NTFS, where the test folder lives).
    let share_all = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    let mut reader = held_open(&path, share_all);
    replace_file(&path, b"new").expect("replaced while held open");
    assert_eq!(fs::read(&path).expect("read"), b"new");
    // The new file held open in turn, for the user file write.
    let mut second_reader = held_open(&path, share_all);
    replace_user_file(&path, b"newer").expect("replaced while held open");
    assert_eq!(fs::read(&path).expect("read"), b"newer");
    // Each reader keeps the content it opened.
    let mut seen = String::new();
    reader.read_to_string(&mut seen).expect("read");
    assert_eq!(seen, "old");
    seen.clear();
    second_reader.read_to_string(&mut seen).expect("read");
    assert_eq!(seen, "new");
    drop(second_reader);
    drop(reader);
    assert_eq!(
        names_in(dir.path()),
        ["profiles.json"],
        "no temporary file left"
    );
}

#[cfg(windows)]
#[test]
fn a_file_held_open_without_delete_sharing_is_left_untouched_and_no_temporary_file_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("profiles.json");
    fs::write(&path, "old").expect("written");
    let reader = held_open(&path, FILE_SHARE_READ | FILE_SHARE_WRITE);
    assert!(replace_file(&path, b"new").is_err());
    assert!(replace_user_file(&path, b"new").is_err());
    drop(reader);
    assert_eq!(fs::read_to_string(&path).expect("read"), "old");
    assert_eq!(
        names_in(dir.path()),
        ["profiles.json"],
        "no temporary file left"
    );
}

#[test]
fn a_shared_gateway_is_reached_by_a_route_and_is_in_no_list_and_no_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge_gateways([gateway("mine", None)]);
    store.save().expect("saves");
    let before = fs::read_to_string(&path).expect("file");
    let mut store = ProfileStore::open(&path).expect("reopens");
    store.set_shared_gateways([
        gateway("shared-team-bastion", Some("elsewhere")),
        // Not in the shared namespace: a shared file cannot name a gateway of its own kind.
        gateway("impostor", None),
    ]);
    let route = store
        .route(Some(&ProfileId::new("shared-team-bastion")))
        .expect("route");
    assert_eq!(route.len(), 1, "a parent outside the namespace is dropped");
    assert!(route[0].parent.is_none());
    assert_eq!(
        store.route(Some(&ProfileId::new("impostor"))),
        Err(RouteError::MissingGateway(ProfileId::new("impostor")))
    );
    // The saved gateways are the saved ones alone, and a save writes nothing of the shared;
    // a change kept by `apply` leaves them reachable.
    assert_eq!(store.gateways().len(), 1);
    store
        .apply(|store| store.merge_gateways([gateway("other", None)]))
        .expect("apply");
    let written = fs::read_to_string(&path).expect("file");
    assert_ne!(written, before);
    assert!(!written.contains("shared-team-bastion"), "{written}");
    assert!(
        store
            .route(Some(&ProfileId::new("shared-team-bastion")))
            .is_ok()
    );
    // Replaced, not added to.
    store.set_shared_gateways([]);
    assert!(
        store
            .route(Some(&ProfileId::new("shared-team-bastion")))
            .is_err()
    );
}

#[test]
fn a_store_from_before_the_untrusted_mark_trusts_only_the_steps_its_approval_covers() {
    use heimdall_core::post_connect::{PostConnect, PostConnectStep};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    let mut imported = profile("imported", "h1");
    imported.post_connect = PostConnect {
        steps: vec![PostConnectStep::new("sudo -i").from_outside()],
        approved: None,
    };
    let mut approved = profile("approved", "h2");
    approved.post_connect.steps = vec![PostConnectStep::new("uptime")];
    // One step approved, one added after: the approval no longer covers the steps.
    let mut changed = profile("changed", "h3");
    changed.post_connect.steps = vec![PostConnectStep::new("ls"), PostConnectStep::new("pwd")];
    store.merge([imported, approved, changed]);
    assert!(store.approve_post_connect(
        &ProfileId::new("approved"),
        &[PostConnectStep::new("uptime")]
    ));
    assert!(store.approve_post_connect(&ProfileId::new("changed"), &[PostConnectStep::new("ls")]));
    store.save().expect("saves");

    // The mark is written explicitly from now on.
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("untrusted = true"), "{text}");
    assert!(text.contains("untrusted = false"), "{text}");

    // The file as an older version wrote it: no mark at all.
    let legacy: String = text
        .lines()
        .filter(|line| !line.contains("untrusted ="))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, legacy).expect("write");
    let reopened = ProfileStore::open(&path).expect("reopens");
    let post_connect = |id: &str| {
        reopened
            .ssh_profiles()
            .iter()
            .find(|profile| profile.id.as_str() == id)
            .expect(id)
            .post_connect
            .clone()
    };
    // Never approved: its steps are untrusted, and stay asked about.
    assert!(post_connect("imported").steps[0].untrusted);
    assert!(post_connect("imported").needs_approval());
    // Approved exactly as they are: no question.
    assert!(!post_connect("approved").steps[0].untrusted);
    assert!(!post_connect("approved").needs_approval());
    // An approval that does not cover the steps trusts none of them.
    assert!(
        post_connect("changed")
            .steps
            .iter()
            .all(|step| step.untrusted)
    );
    assert!(post_connect("changed").needs_approval());
}

#[test]
fn a_version_15_file_reads_with_no_far_end_os_and_a_declared_one_is_kept() {
    use heimdall_core::profile::RemoteOs;
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 15\n\n[[ssh]]\nid = \"a\"\nname = \"A\"\nhost = \"h\"\nport = 22\n\n[[telnet]]\nid = \"t\"\nname = \"T\"\nhost = \"sw\"\nport = 23\n",
    )
    .expect("writes");
    let mut store = ProfileStore::open(&path).expect("a version 15 file opens");
    let (ssh_id, telnet_id) = (ProfileId::new("a"), ProfileId::new("t"));
    assert_eq!(store.ssh_profiles()[0].remote_os, RemoteOs::Auto);
    assert_eq!(store.remote_os_of(&ssh_id), RemoteOs::Auto);
    let mut windows = profile("a", "h");
    windows.remote_os = RemoteOs::Windows;
    let mut linux = telnet("t");
    linux.remote_os = RemoteOs::Linux;
    store.merge([windows]);
    store.merge_telnet([linux]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.starts_with("version = 16\n"), "{text}");
    assert!(text.contains("remote_os = \"windows\""), "{text}");
    assert!(text.contains("remote_os = \"linux\""), "{text}");
    let back = ProfileStore::open(&path).expect("reopens");
    assert_eq!(back.remote_os_of(&ssh_id), RemoteOs::Windows);
    assert_eq!(back.remote_os_of(&telnet_id), RemoteOs::Linux);
    assert_eq!(back.remote_os_of(&ProfileId::new("none")), RemoteOs::Auto);
}

#[test]
fn an_undeclared_far_end_os_is_not_written() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "h")]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(!text.contains("remote_os"), "{text}");
}
