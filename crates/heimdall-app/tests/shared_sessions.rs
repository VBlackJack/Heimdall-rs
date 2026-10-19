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

//! Shared sessions, as the C# `SharedSessionProvider`: a `MobaXterm` file read for its SSH
//! and RDP sessions, in memory and read-only, followed when it changes, and refused when it
//! is not a file Heimdall can read.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, SystemTime};

use heimdall_app::shared_sessions::{
    SharedSessions, SharedTarget, SourceProblem, SourceStatus, Variables, expand,
};
use heimdall_app::{App, AppConfig, Message};
use heimdall_core::settings::{Settings, settings_path};
use heimdall_core::shared_sessions::{MAX_SOURCE_FILE_BYTES, SharedSource, is_shared};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

const TEAM: &str = "[Bookmarks]\r\nSubRep=Prod\\Web\r\nImgNum=42\r\n\
web01= #109#0%web01.example.com%2222%deploy%%-1%-1%%%%%0%0%0%C:\\Keys\\id_rsa%%-1%0%0%0%%1080%%0\r\n\
desk= #91#4%desk.example.com%3390%admin%0%-1%-1%\r\n\
files= #140#5%files.example.com%22%bob%\r\n\
bad host= #109#0%evil host;rm -rf%22%root%\r\n\
telnet= #98#1%tel.example.com%23%%\r\n\
[Passwords]\r\ndeploy@web01=SECRET-PASSWORD-VALUE\r\n";

fn source(id: &str, path: &Path) -> SharedSource {
    SharedSource {
        id: id.to_owned(),
        name: "Team".to_owned(),
        path: path.to_string_lossy().into_owned(),
        enabled: true,
    }
}

/// A moment after which a file just written has settled.
fn later() -> SystemTime {
    SystemTime::now() + Duration::from_secs(60)
}

fn loaded(dir: &Path, text: &str) -> (SharedSessions, Vec<SharedSource>) {
    let file = dir.join("team.mxtsessions");
    std::fs::write(&file, text).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = SharedSessions::default();
    shared.load(&sources, &Variables::default());
    (shared, sources)
}

#[test]
fn a_good_file_gives_its_ssh_and_rdp_sessions_and_says_what_it_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), TEAM);
    let state = &shared.sources()[0];
    assert_eq!(state.status, SourceStatus::Loaded);
    assert_eq!(state.sessions, 2, "{state:?}");
    // The SFTP session, the Telnet one and the one with a host that is not one.
    assert_eq!(state.refused, 3);
    assert_eq!(state.ignored_passwords, 1);
    let sessions = shared.sessions_of("team");
    let SharedTarget::Ssh(ssh) = &sessions[0].target else {
        panic!("{sessions:?}");
    };
    assert_eq!((ssh.host.as_str(), ssh.port), ("web01.example.com", 2222));
    assert_eq!(ssh.username.as_deref(), Some("deploy"));
    assert_eq!(sessions[0].folder.as_deref(), Some("Prod/Web"));
    assert!(is_shared(sessions[0].id.as_str()));
    assert!(sessions[0].id.as_str().starts_with("shared-team-"));
    let SharedTarget::Rdp(rdp) = &sessions[1].target else {
        panic!("{sessions:?}");
    };
    assert_eq!((rdp.host.as_str(), rdp.port), ("desk.example.com", 3390));
}

#[test]
fn nothing_the_file_could_run_or_trust_comes_with_a_session() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), TEAM);
    let SharedTarget::Ssh(ssh) = &shared.sessions_of("team")[0].target else {
        panic!("an SSH session");
    };
    // The file names a key, a compression and an agent: none of it is the session's.
    assert!(ssh.key_path.is_none());
    assert!(ssh.gateway.is_none());
    assert!(ssh.post_connect.is_empty());
    assert!(!ssh.forward_agent && !ssh.compression);
    assert!(ssh.group.is_none(), "no saved folder's defaults reach it");
    assert!(ssh.vault_entry.is_none());
    // A stored password is counted and never read.
    let held = format!("{shared:?}");
    assert!(!held.contains("SECRET-PASSWORD-VALUE"), "{held}");
}

#[test]
fn the_file_is_only_read_and_a_session_handed_out_is_a_copy() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shared, sources) = loaded(dir.path(), TEAM);
    let file = dir.path().join("team.mxtsessions");
    let before = std::fs::read(&file).expect("read");
    let id = shared.sessions_of("team")[0].id.clone();
    let mut copy = shared.find(&id).expect("found");
    if let SharedTarget::Ssh(profile) = &mut copy.target {
        profile.host = "changed.example.com".to_owned();
    }
    assert_ne!(shared.find(&id), Some(copy), "the copy is not the held one");
    shared.refresh(&sources, &Variables::default(), later());
    assert_eq!(std::fs::read(&file).expect("read"), before);
    assert!(
        !std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .readonly(),
        "the file's permissions are left as they were"
    );
}

#[test]
fn a_file_that_is_not_usable_loads_nothing_and_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), "this is not a session file\n\u{0}\u{1}");
    let state = &shared.sources()[0];
    assert_eq!(state.status, SourceStatus::Failed(SourceProblem::Empty));
    assert_eq!(shared.sessions().count(), 0);

    let mut missing = SharedSessions::default();
    missing.load(
        &[source("gone", &dir.path().join("nowhere.ini"))],
        &Variables::default(),
    );
    assert_eq!(
        missing.sources()[0].status,
        SourceStatus::Failed(SourceProblem::Missing)
    );

    let big = dir.path().join("big.ini");
    let file = std::fs::File::create(&big).expect("create");
    file.set_len(MAX_SOURCE_FILE_BYTES + 1).expect("length");
    let mut oversized = SharedSessions::default();
    oversized.load(&[source("big", &big)], &Variables::default());
    assert_eq!(
        oversized.sources()[0].status,
        SourceStatus::Failed(SourceProblem::TooLarge)
    );
    assert_eq!(oversized.sessions().count(), 0);
}

#[test]
fn a_source_with_a_bad_identifier_a_repeated_one_or_no_path_is_reported() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let mut off = source("off", &file);
    off.enabled = false;
    let mut no_path = source("nopath", &file);
    no_path.path = "  ".to_owned();
    let sources = vec![
        source("Bad-Id", &file),
        source("team", &file),
        source("team", &file),
        off,
        no_path,
    ];
    let mut shared = SharedSessions::default();
    shared.load(&sources, &Variables::default());
    let statuses: Vec<_> = shared.sources().iter().map(|s| s.status.clone()).collect();
    assert_eq!(
        statuses,
        vec![
            SourceStatus::Failed(SourceProblem::InvalidId),
            SourceStatus::Loaded,
            SourceStatus::Failed(SourceProblem::DuplicateId),
            SourceStatus::Disabled,
            SourceStatus::Failed(SourceProblem::NoPath),
        ]
    );
    assert_eq!(shared.sessions().count(), 2, "only the good source's");
}

#[test]
fn a_changed_file_is_read_again_and_a_worse_one_keeps_the_previous_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shared, sources) = loaded(dir.path(), TEAM);
    let file = dir.path().join("team.mxtsessions");
    assert!(
        !shared.refresh(&sources, &Variables::default(), later()),
        "nothing changed"
    );

    std::fs::write(
        &file,
        "[Bookmarks]\r\nonly= #109#0%only.example.com%22%me%\r\nsecond= #109#0%second.example.com%22%me%\r\n",
    )
    .expect("write");
    // Just written: left to settle.
    assert!(!shared.refresh(&sources, &Variables::default(), SystemTime::now()));
    assert_eq!(shared.sources()[0].sessions, 2);
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    let names: Vec<_> = shared.sessions().map(|s| s.name().to_owned()).collect();
    assert_eq!(names, ["only", "second"]);

    // A file that no longer parses: the sessions held stay, with the reason.
    std::fs::write(&file, "garbage, garbage and more garbage").expect("write");
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    let state = &shared.sources()[0];
    assert_eq!(state.status, SourceStatus::Loaded);
    assert_eq!(state.kept_previous, Some(SourceProblem::Empty));
    assert_eq!(shared.sessions().count(), 2);

    // Gone: nothing stays connectable from a file that is not there.
    std::fs::remove_file(&file).expect("remove");
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    assert_eq!(
        shared.sources()[0].status,
        SourceStatus::Failed(SourceProblem::Missing)
    );
    assert_eq!(shared.sessions().count(), 0);

    // Back and good: the notice goes.
    std::fs::write(&file, TEAM).expect("write");
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    assert_eq!(shared.sources()[0].kept_previous, None);
    assert_eq!(shared.sessions().count(), 2);
}

#[test]
fn a_source_removed_from_the_settings_takes_its_sessions_with_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shared, _) = loaded(dir.path(), TEAM);
    assert!(shared.refresh(&[], &Variables::default(), later()));
    assert!(shared.is_empty());
    assert_eq!(shared.sessions().count(), 0);
}

#[test]
fn variables_of_a_path_are_expanded_and_an_unset_one_stays_as_written() {
    // SAFETY-free: a variable every system sets.
    let path = std::env::var("PATH").expect("PATH");
    assert_eq!(expand("%PATH%"), path);
    assert_eq!(
        expand(" %HEIMDALL_NO_SUCH_VARIABLE%\\x "),
        "%HEIMDALL_NO_SUCH_VARIABLE%\\x"
    );
    assert_eq!(expand("100%"), "100%");
}

fn app_with_source(dir: &Path) -> App {
    let file = dir.join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let profiles_file = dir.join("profiles.toml");
    let settings = Settings {
        shared_sources: vec![source("team", &file)],
        ..Settings::default()
    };
    settings
        .save(&settings_path(&profiles_file))
        .expect("settings");
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

#[test]
fn the_app_holds_them_apart_from_the_saved_profiles_and_opens_one_never_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_source(dir.path());
    assert_eq!(app.shared_sessions().sessions().count(), 2);
    assert!(app.watches_shared_sources());
    // Not among the saved profiles: no menu that edits or deletes one can reach them.
    assert!(app.profile_summaries().is_empty());
    assert!(app.profiles().is_empty());
    let id = app
        .shared_sessions()
        .sessions()
        .next()
        .expect("one")
        .id
        .clone();
    assert!(app.profile_summary(&id).is_none());

    let before = app.tabs.len();
    let effects = app.update(Message::ConnectShared(id));
    assert!(!effects.is_empty(), "a connection starts");
    assert_eq!(app.tabs.len(), before + 1);
    assert!(app.profiles().is_empty(), "nothing is saved by opening it");
}

#[test]
fn an_unknown_shared_identifier_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_source(dir.path());
    let effects = app.update(Message::ConnectShared(
        heimdall_core::profile::ProfileId::new("shared-team-0000"),
    ));
    assert!(effects.is_empty());
    assert!(app.tabs.is_empty());
}

#[test]
fn a_crafted_folder_cannot_make_the_reader_allocate_without_bound() {
    // An 8 MiB folder and a thousand tiny sessions: copied into each, that is 8 GiB.
    let mut text = String::from("[Bookmarks]\nSubRep=");
    text.push_str(&"F".repeat(8 * 1024 * 1024));
    text.push('\n');
    for index in 0..1000 {
        let _ = writeln!(text, "s{index}= #109#0%h{index}.example.com%22%u%");
    }
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), &text);
    // The section whose folder is too long is not taken at all.
    assert_eq!(
        shared.sources()[0].status,
        SourceStatus::Failed(SourceProblem::Empty)
    );
    assert_eq!(shared.sessions().count(), 0);

    // A long name is left out on its own, the others are kept and the notice counts it.
    let long = "n".repeat(100_000);
    let text = format!(
        "[Bookmarks]\nSubRep=Ok
{long}= #109#0%a.example.com%22%u%\nfine= #109#0%b.example.com%22%u%\n"
    );
    let (shared, _) = loaded(dir.path(), &text);
    assert_eq!(shared.sessions().count(), 1);
    assert_eq!(shared.sources()[0].skipped_entries, 1);
}

#[test]
fn a_file_with_more_entries_than_a_source_may_hold_is_cut_and_says_so() {
    let mut text = String::from("[Bookmarks]\n");
    for index in 0..30_000 {
        let _ = writeln!(text, "s{index}= #109#0%h{index}.example.com%22%u%");
    }
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), &text);
    let state = &shared.sources()[0];
    assert!(state.truncated, "{state:?}");
    assert!(shared.sessions().count() <= 5000);
}

#[test]
fn malformed_entries_next_to_good_ones_are_skipped_and_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(
        dir.path(),
        "[Bookmarks]
good= #109#0%ok.example.com%22%u%
broken=nonsense
nohost= #109#0%%22%u%
",
    );
    let state = &shared.sources()[0];
    assert_eq!(state.sessions, 1);
    assert_eq!(state.skipped_entries, 2);
}

#[test]
fn stored_passwords_are_counted_and_no_value_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(
        dir.path(),
        "[Bookmarks]
good= #109#0%ok.example.com%22%u%
[Passwords]
a@b=TOPSECRET1
c@d=TOPSECRET2
[Credentials]
e=TOPSECRET3
",
    );
    assert_eq!(shared.sources()[0].ignored_passwords, 3);
    let held = format!("{shared:?}");
    assert!(!held.contains("TOPSECRET"), "{held}");
}

#[test]
fn the_users_variables_fill_in_a_login_as_the_file_import_does() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("vars.mxtsessions");
    std::fs::write(
        &file,
        "[Bookmarks]
web= #109#0%web.example.com%22%[ADMIN]%
",
    )
    .expect("write");
    let sources = vec![source("vars", &file)];
    let variables = std::collections::BTreeMap::from([("ADMIN".to_owned(), "alice".to_owned())]);
    let mut shared = SharedSessions::default();
    shared.load(&sources, &variables);
    let SharedTarget::Ssh(ssh) = &shared.sessions_of("vars")[0].target else {
        panic!("an SSH session");
    };
    assert_eq!(ssh.username.as_deref(), Some("alice"));
}
