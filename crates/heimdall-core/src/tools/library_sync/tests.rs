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

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use serde_json::{Value, json};

use super::git::{self, args};
use super::settings::{
    DEFAULT_BRANCH, PLACEHOLDER_AUTHOR_EMAIL, PLACEHOLDER_AUTHOR_NAME, mask_credentials,
};
use super::*;
use crate::tools::library_transfer;
use crate::tools::runbooks::{Runbook, RunbookStore, Step};
use crate::tools::user_commands::{
    Criticality, Draft, LIBRARY_FILE, LibraryError, Quoting, TemplatePlatform, UserLibrary,
    UserParameter, UserPlatform, UserTemplate,
};

/// The operations share one lock; the tests that run them take turns.
static TURN: Mutex<()> = Mutex::new(());

fn settings(url: &str, folder: &Path) -> SyncSettings {
    SyncSettings {
        remote_url: url.to_owned(),
        local_path: folder.to_string_lossy().into_owned(),
        branch: "main".to_owned(),
        author_name: "Tester".to_owned(),
        author_email: "tester@example.com".to_owned(),
    }
}

/// An absolute path that exists on no computer in particular.
fn absolute(name: &str) -> std::path::PathBuf {
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    Path::new(root).join(name)
}

fn valid() -> SyncSettings {
    let folder = absolute("heimdall-sync-settings-test");
    settings("https://example.com/team/library.git", &folder)
}

fn text(arguments: &[OsString]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

// ----- settings -----

#[test]
fn the_urls_of_the_three_allowed_transports_are_taken() {
    for url in [
        "https://example.com/team/library.git",
        "https://user:token@example.com/team/library.git",
        "HTTPS://example.com/library.git",
        "ssh://git@example.com/team/library.git",
        "file:///srv/library.git",
        "file:///C:/repos/library.git",
        "git@example.com:team/library.git",
        "example.com:team/library.git",
    ] {
        let mut set = valid();
        set.remote_url = url.to_owned();
        assert!(set.validate().is_ok(), "{url}");
    }
}

#[test]
fn a_url_that_could_run_a_command_or_be_an_option_is_refused() {
    let refused = |url: &str| {
        let mut set = valid();
        set.remote_url = url.to_owned();
        set.validate().unwrap_err()
    };
    // Options.
    assert_eq!(refused("-oProxyCommand=evil"), SettingsIssue::UrlOption);
    assert_eq!(refused("--upload-pack=evil"), SettingsIssue::UrlOption);
    // The transport helpers run a command.
    assert_eq!(refused("ext::sh -c touch% x"), SettingsIssue::UrlHidden);
    assert_eq!(refused("ext::sh%20-c%20evil"), SettingsIssue::UrlScheme);
    assert_eq!(refused("fd::17/foo"), SettingsIssue::UrlScheme);
    assert_eq!(refused("ext://whatever"), SettingsIssue::UrlScheme);
    // Other schemes.
    assert_eq!(
        refused("http://example.com/x.git"),
        SettingsIssue::UrlScheme
    );
    assert_eq!(refused("git://example.com/x.git"), SettingsIssue::UrlScheme);
    assert_eq!(refused("ftp://example.com/x.git"), SettingsIssue::UrlScheme);
    // Not a URL.
    assert_eq!(refused(""), SettingsIssue::UrlMissing);
    assert_eq!(refused("   "), SettingsIssue::UrlMissing);
    assert_eq!(refused("/srv/library.git"), SettingsIssue::UrlFormat);
    assert_eq!(refused("C:\\repos\\library.git"), SettingsIssue::UrlFormat);
    assert_eq!(refused("https://"), SettingsIssue::UrlFormat);
    // Hidden and spaced.
    assert_eq!(
        refused("https://exa\u{0}mple.com/x"),
        SettingsIssue::UrlHidden
    );
    assert_eq!(
        refused("https://example.com/x\n--bad"),
        SettingsIssue::UrlHidden
    );
    assert_eq!(
        refused("https://example.com/\u{202e}x"),
        SettingsIssue::UrlHidden
    );
    assert_eq!(refused("https://example.com/a b"), SettingsIssue::UrlHidden);
    let long = format!(
        "https://example.com/{}",
        "a".repeat(super::settings::MAX_URL_LENGTH)
    );
    assert_eq!(refused(&long), SettingsIssue::UrlTooLong);
}

#[test]
fn a_branch_is_a_name_git_takes_and_never_an_option() {
    let branch = |name: &str| {
        let mut set = valid();
        set.branch = name.to_owned();
        set.validate()
    };
    for good in ["main", "release/2026.10", "feature_x-1", "v1.2"] {
        assert!(branch(good).is_ok(), "{good}");
    }
    for bad in [
        "--upload-pack=evil",
        "-b",
        "a b",
        "a..b",
        "a//b",
        "/a",
        "a/",
        ".a",
        "a.",
        "a.lock",
        "a/.b",
        "a~1",
        "a^",
        "a:b",
        "a?",
        "a*",
        "a[",
        "a\\b",
        "a@{1}",
        "a;touch x",
        "$(x)",
    ] {
        assert_eq!(
            branch(bad).unwrap_err(),
            SettingsIssue::BranchFormat,
            "{bad}"
        );
    }
    assert_eq!(branch("").unwrap_err(), SettingsIssue::BranchMissing);
}

#[test]
fn a_folder_is_absolute_and_stays_where_it_says() {
    let folder = |path: &str| {
        let mut set = valid();
        set.local_path = path.to_owned();
        set.validate()
    };
    let absolute = absolute("library");
    assert!(folder(&absolute.to_string_lossy()).is_ok());
    assert_eq!(folder("").unwrap_err(), SettingsIssue::PathMissing);
    assert_eq!(folder("library").unwrap_err(), SettingsIssue::PathRelative);
    assert_eq!(folder("-library").unwrap_err(), SettingsIssue::PathHidden);
    let parent = format!("{}/../elsewhere", absolute.to_string_lossy());
    assert_eq!(folder(&parent).unwrap_err(), SettingsIssue::PathParent);
    let hidden = format!("{}\u{0}x", absolute.to_string_lossy());
    assert_eq!(folder(&hidden).unwrap_err(), SettingsIssue::PathHidden);
}

#[test]
fn an_author_is_a_name_and_an_address_that_cannot_be_an_option() {
    let author = |name: &str, email: &str| {
        let mut set = valid();
        set.author_name = name.to_owned();
        set.author_email = email.to_owned();
        set.validate()
    };
    assert!(author("Julien Bombled", "julien@example.com").is_ok());
    assert_eq!(author("", "a@b.c").unwrap_err(), SettingsIssue::AuthorName);
    assert_eq!(
        author("-x", "a@b.c").unwrap_err(),
        SettingsIssue::AuthorName
    );
    assert_eq!(
        author("A\nB", "a@b.c").unwrap_err(),
        SettingsIssue::AuthorName
    );
    assert_eq!(
        author("A <b>", "a@b.c").unwrap_err(),
        SettingsIssue::AuthorName
    );
    assert_eq!(
        author("A", "nobody").unwrap_err(),
        SettingsIssue::AuthorEmail
    );
    assert_eq!(
        author("A", "-a@b.c").unwrap_err(),
        SettingsIssue::AuthorEmail
    );
    assert_eq!(
        author("A", "a b@c.d").unwrap_err(),
        SettingsIssue::AuthorEmail
    );
}

#[test]
fn the_git_identity_fills_the_placeholders_and_nothing_chosen() {
    let mut set = SyncSettings::default();
    assert_eq!(set.author_name, PLACEHOLDER_AUTHOR_NAME);
    assert_eq!(set.author_email, PLACEHOLDER_AUTHOR_EMAIL);
    assert_eq!(set.branch, DEFAULT_BRANCH);
    assert!(set.author_is_placeholder());
    assert!(set.take_identity(Some(" Julien "), Some("julien@example.com")));
    assert_eq!(set.author_name, "Julien");
    assert_eq!(set.author_email, "julien@example.com");
    assert!(!set.author_is_placeholder());
    // Chosen values stay.
    assert!(!set.take_identity(Some("Someone"), Some("else@example.com")));
    assert_eq!(set.author_name, "Julien");
    // Each field on its own.
    let mut half = SyncSettings {
        author_name: "Chosen".to_owned(),
        ..SyncSettings::default()
    };
    assert!(half.take_identity(Some("Git"), Some("git@example.com")));
    assert_eq!(half.author_name, "Chosen");
    assert_eq!(half.author_email, "git@example.com");
    // A value Git has that would be refused is not taken.
    let mut bad = SyncSettings::default();
    assert!(!bad.take_identity(Some("-x"), Some("nobody")));
    assert!(bad.author_is_placeholder());
    assert!(!bad.take_identity(None, None));
}

#[test]
fn the_credentials_of_a_url_never_reach_a_message() {
    assert_eq!(
        mask_credentials("fatal: unable to access 'https://user:tok3n@example.com/x.git/': 403"),
        "fatal: unable to access 'https://***@example.com/x.git/': 403"
    );
    assert_eq!(
        mask_credentials("https://tok3n@example.com and ssh://git:pw@host:22/x"),
        "https://***@example.com and ssh://***@host:22/x"
    );
    assert_eq!(
        mask_credentials("https://example.com/x.git"),
        "https://example.com/x.git"
    );
    assert_eq!(mask_credentials("no url here"), "no url here");
    // A failure of git quoting the URL is masked in what is shown.
    let failure =
        git::classify("fatal: Authentication failed for 'https://u:secret@example.com/x'");
    let SyncFailure::Authentication(detail) = failure else {
        panic!("not an authentication failure");
    };
    assert!(!detail.contains("secret"), "{detail}");
    assert!(detail.contains("***@example.com"));
}

// ----- the state file -----

#[test]
fn the_state_round_trips_and_a_foreign_file_is_not_written_over() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(SETTINGS_FILE);
    assert_eq!(SyncState::load(&path).expect("none"), SyncState::default());
    let mut state = SyncState::default();
    state.settings = valid();
    state.runbook_bases.insert("abc".to_owned(), u64::MAX);
    state.save(&path).expect("saved");
    assert_eq!(SyncState::load(&path).expect("read"), state);
    std::fs::write(&path, "not json").expect("written");
    assert!(matches!(SyncState::load(&path), Err(StateError::Format)));
}

// ----- the arguments of git -----

#[test]
fn what_a_person_typed_is_always_after_a_double_dash() {
    // Whatever the value, a call that takes one puts it after `--`, or inside a longer
    // argument that cannot start with `-`.
    for hostile in [
        "--upload-pack=evil",
        "-oProxyCommand=evil",
        "a b; touch x",
        "$(x)",
    ] {
        let calls = [
            args::add_origin(hostile),
            args::remote_branch(hostile),
            args::fetch(hostile),
            args::push(hostile),
        ];
        for call in calls {
            let words = text(&call);
            let dashes = words.iter().position(|word| word == "--");
            let at = words
                .iter()
                .position(|word| word.contains(hostile))
                .expect("the value is passed");
            assert!(
                dashes.is_some_and(|dashes| dashes < at) || !words[at].starts_with('-'),
                "{words:?}"
            );
            // Before the `--` only fixed words.
            if let Some(dashes) = dashes {
                assert!(words[..dashes].iter().all(|word| !word.contains(hostile)));
            }
        }
    }
    // The URL is a word of its own, after the `--` and the remote's name.
    let add = text(&args::add_origin("https://example.com/x.git"));
    assert_eq!(
        add,
        ["remote", "add", "--", "origin", "https://example.com/x.git"]
    );
    let push = text(&args::push("main"));
    assert_eq!(
        push,
        [
            "push",
            "--quiet",
            "--no-verify",
            "--",
            "origin",
            "HEAD:refs/heads/main"
        ]
    );
    // The file is named after `--` too.
    assert_eq!(text(&args::stage()), ["add", "--", REPOSITORY_FILE]);
    assert!(text(&args::commit("m")).ends_with(&["--".to_owned(), REPOSITORY_FILE.to_owned()]));
}

#[test]
fn no_call_forces_a_push_or_runs_a_shell() {
    let calls = [
        args::init(),
        args::point_head("main"),
        args::add_origin("https://example.com/x.git"),
        args::origin_url(),
        args::current_branch(),
        args::remote_branch("main"),
        args::fetch("main"),
        args::head_exists(),
        args::behind_count(),
        args::fetched_count(),
        args::ahead_count(),
        args::contains_fetched(),
        args::fetched_size(),
        args::fetched_content(),
        args::fast_forward(),
        args::move_to_fetched(),
        args::stage(),
        args::staged_changes(),
        args::commit("message"),
        args::push("main"),
        args::global_value("user.name"),
    ];
    for call in calls {
        let words = text(&call);
        for forbidden in [
            "--force",
            "-f",
            "--force-with-lease",
            "-c",
            "sh",
            "cmd",
            "/c",
            "-C",
        ] {
            assert!(!words.iter().any(|word| word == forbidden), "{words:?}");
        }
    }
}

#[test]
fn every_call_is_narrowed_to_three_transports_without_hooks_prompts_or_editor() {
    let hooks = Path::new("/empty-hooks");
    let global = text(&git::global_arguments(
        hooks,
        Some(("Tester", "t@example.com")),
    ));
    let settings: Vec<&String> = global
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, word)| word)
        .collect();
    assert!(
        global.iter().step_by(2).all(|flag| flag == "-c"),
        "{global:?}"
    );
    assert!(
        settings
            .iter()
            .any(|word| word.starts_with("core.hooksPath="))
    );
    for needed in [
        "protocol.ext.allow=never",
        "core.fsmonitor=false",
        "transfer.fsckObjects=true",
        "commit.gpgsign=false",
        "user.name=Tester",
        "user.email=t@example.com",
    ] {
        assert!(settings.iter().any(|word| *word == needed), "{needed}");
    }
    // The identity is optional.
    let anonymous = text(&git::global_arguments(hooks, None));
    assert!(!anonymous.iter().any(|word| word.starts_with("user.")));

    let (set, removed) = git::environment(false);
    let value = |name: &str| set.iter().find(|(key, _)| *key == name).map(|(_, v)| *v);
    assert_eq!(value("GIT_TERMINAL_PROMPT"), Some("0"));
    assert_eq!(value("GCM_INTERACTIVE"), Some("never"));
    assert_eq!(value("GIT_ALLOW_PROTOCOL"), Some("https:ssh:file"));
    assert_eq!(value("GIT_EDITOR"), Some("true"));
    assert_eq!(value("LC_ALL"), Some("C"));
    assert_eq!(value("GIT_SSH_COMMAND"), Some("ssh -o BatchMode=yes"));
    assert!(removed.contains(&"GIT_DIR") && removed.contains(&"GIT_WORK_TREE"));
    // The person's own ssh command is theirs.
    let (own, _) = git::environment(true);
    assert!(!own.iter().any(|(key, _)| *key == "GIT_SSH_COMMAND"));
    // The protocols allowed are exactly the schemes the settings take.
    let allowed: Vec<&str> = git::ALLOWED_PROTOCOLS.split(':').collect();
    assert_eq!(allowed, super::settings::ALLOWED_SCHEMES);
}

#[test]
fn git_is_looked_for_in_the_absolute_directories_of_the_path_only() {
    let first = tempfile::tempdir().expect("first");
    let second = tempfile::tempdir().expect("second");
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    std::fs::write(second.path().join(name), "").expect("a git");
    // A relative directory, and the current directory by its empty name, are not searched.
    let joined = std::env::join_paths([
        Path::new("."),
        Path::new(""),
        Path::new("bin"),
        first.path(),
        second.path(),
    ])
    .expect("a path");
    let found = git::resolve_in(&joined, cfg!(windows)).expect("found");
    assert_eq!(found, second.path().join(name));
    let none = std::env::join_paths([first.path()]).expect("a path");
    assert_eq!(git::resolve_in(&none, cfg!(windows)), None);
}

#[test]
fn what_git_says_is_told_by_its_words() {
    assert!(matches!(
        git::classify("fatal: could not read Username for 'https://x': terminal prompts disabled"),
        SyncFailure::Authentication(_)
    ));
    assert!(matches!(
        git::classify("git@x: Permission denied (publickey)."),
        SyncFailure::Authentication(_)
    ));
    assert!(matches!(
        git::classify("fatal: repository 'https://x/y.git/' not found"),
        SyncFailure::RemoteNotFound(_)
    ));
    assert!(matches!(
        git::classify("fatal: unable to access 'https://x/': Could not resolve host: x"),
        SyncFailure::Network(_)
    ));
    assert_eq!(
        git::classify("! [rejected] HEAD -> main (fetch first)\nerror: failed to push some refs"),
        SyncFailure::PushRejected
    );
    let SyncFailure::Git(detail) = git::classify("fatal:\tsomething\n\u{1b}[31mnew") else {
        panic!("not a plain failure");
    };
    assert_eq!(detail, "fatal: something [31mnew");
    let long = "x".repeat(5000);
    let SyncFailure::Git(detail) = git::classify(&long) else {
        panic!("not a plain failure");
    };
    assert_eq!(detail.chars().count(), 400);
}

// ----- merging the actions -----

fn action(id: &str, title: &str, updated: &str, pattern: &str, level: &str) -> Value {
    json!({
        "id": id,
        "publicId": id,
        "title": title,
        "description": "",
        "category": "Sync",
        "platform": 1,
        "level": level,
        "tags": [],
        "linuxCommandTemplate": {
            "platform": 1,
            "name": "",
            "commandPattern": pattern,
            "parameters": []
        },
        "createdAt": "2026-01-01T00:00:00.0000000Z",
        "updatedAt": updated,
        "isUserCreated": true
    })
}

fn file(actions: &[Value]) -> String {
    json!({ "schemaVersion": "1.0", "actions": actions }).to_string()
}

const OLD: &str = "2026-03-01T00:00:00.0000000Z";
const NEW: &str = "2026-06-01T00:00:00.0000000Z";
const NEWER: &str = "2026-09-01T00:00:00.0000000Z";

#[test]
fn a_newer_remote_copy_replaces_the_local_one_and_keeps_its_own_timestamp() {
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[action("a1", "Ping", OLD, "ping {x}", "Info")]))
        .expect("first");
    let report = library
        .sync_merge(&file(&[action(
            "a1",
            "Ping 2",
            NEW,
            "ping -c1 {x}",
            "Info",
        )]))
        .expect("second");
    assert_eq!((report.imported, report.updated), (0, 1));
    assert!(report.kept_local.is_empty());
    let kept = library.get("a1").expect("kept");
    assert_eq!(kept.title, "Ping 2");
    // The file's timestamp, not the time of the merge: the next sync must not read the
    // copy as a local edit.
    assert_eq!(kept.updated_at.as_deref(), Some(NEW));
}

#[test]
fn a_local_copy_newer_than_the_remote_one_is_kept_and_named() {
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[
            action("a1", "Edited here", NEWER, "ping {x}", "Info"),
            action("a2", "Also here", NEWER, "ls", "Info"),
            action("a3", "Older here", OLD, "pwd", "Info"),
        ]))
        .expect("seed");
    let report = library
        .sync_merge(&file(&[
            action("a1", "Edited there", NEW, "ping -c1 {x}", "Info"),
            action("a2", "Also there", NEW, "ls -l", "Info"),
            action("a3", "Newer there", NEW, "pwd -P", "Info"),
        ]))
        .expect("merge");
    assert_eq!((report.imported, report.updated), (0, 1));
    assert_eq!(report.kept_local, ["Edited here", "Also here"]);
    assert_eq!(library.get("a1").expect("a1").title, "Edited here");
    assert_eq!(library.get("a3").expect("a3").title, "Newer there");
}

#[test]
fn a_library_read_back_keeps_the_dates_it_was_written_with() {
    // Re-dated at every start, every action would look newer here than in the repository.
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(LIBRARY_FILE);
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[action("a1", "One", OLD, "ls", "Info")]))
        .expect("seed");
    library.save(&path).expect("saved");
    let loaded = UserLibrary::load(&path).expect("loaded");
    assert_eq!(
        loaded.get("a1").expect("a1").updated_at.as_deref(),
        Some(OLD)
    );
    // So a remote copy newer than that one still wins.
    let mut again = loaded;
    let report = again
        .sync_merge(&file(&[action("a1", "One v2", NEW, "ls -l", "Info")]))
        .expect("merge");
    assert_eq!((report.updated, report.kept_local.len()), (1, 0));
}

#[test]
fn a_file_merged_twice_changes_nothing_and_reports_no_conflict() {
    let pulled = file(&[
        action("a1", "One", OLD, "ls", "Info"),
        action("a2", "Two", NEW, "pwd", "Run"),
    ]);
    let mut library = UserLibrary::default();
    let first = library.sync_merge(&pulled).expect("first");
    assert_eq!(first.imported, 2);
    let before = library.clone();
    let again = library.sync_merge(&pulled).expect("again");
    assert_eq!(
        (again.imported, again.updated, again.skipped),
        (0, 0, 0),
        "an unchanged file is not an update"
    );
    assert!(again.kept_local.is_empty());
    assert_eq!(library, before);
}

#[test]
fn a_local_copy_with_the_same_content_is_not_a_conflict() {
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[action("a1", "Same", NEWER, "ls", "Info")]))
        .expect("seed");
    let report = library
        .sync_merge(&file(&[action("a1", "Same", OLD, "ls", "Info")]))
        .expect("merge");
    assert!(report.kept_local.is_empty());
    assert_eq!(report.updated, 0);
}

#[test]
fn a_pulled_action_is_held_to_the_rules_of_an_untrusted_file() {
    let mut library = UserLibrary::default();
    let report = library
        .sync_merge(&file(&[
            action("a1", "Breaks a line", NEW, "ls\nrm -rf /", "Info"),
            action("a2", "Hides", NEW, "ls \u{202e}x", "Info"),
            action("a3", "Fine", NEW, "ls", "Info"),
        ]))
        .expect("merge");
    assert_eq!((report.imported, report.skipped), (1, 2));
    assert_eq!(
        report
            .first_skipped
            .as_ref()
            .map(|(title, _)| title.as_str()),
        Some("Breaks a line")
    );
    assert!(library.get("a1").is_none() && library.get("a3").is_some());
    // A file that is not an envelope is refused whole.
    assert!(matches!(
        library.sync_merge("[1, 2]"),
        Err(LibraryError::Format)
    ));
}

#[test]
fn run_and_dangerous_actions_survive_the_trip() {
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[
            action("a1", "Reads", NEW, "ls", "Info"),
            action("a2", "Changes", NEW, "touch x", "Run"),
            action("a3", "Destroys", NEW, "rm -r x", "Dangerous"),
        ]))
        .expect("merge");
    let level = |id: &str| library.get(id).expect("kept").level;
    assert_eq!(level("a1"), Criticality::Info);
    assert_eq!(level("a2"), Criticality::Run);
    assert_eq!(level("a3"), Criticality::Dangerous);
    // And out and in again, through the export a push writes.
    let mut other = UserLibrary::default();
    let report = other.sync_merge(&library.export()).expect("round trip");
    assert_eq!((report.imported, report.skipped), (3, 0));
    assert_eq!(other.get("a2").expect("a2").level, Criticality::Run);
    assert_eq!(other.get("a3").expect("a3").level, Criticality::Dangerous);
}

#[test]
fn templates_with_a_choice_a_number_and_a_drive_letter_come_back_whole() {
    let typed = |name: &str, kind: &str| {
        let mut parameter = UserParameter::new(name);
        parameter.kind = kind.to_owned();
        parameter
    };
    let mut level = typed("level", "choice");
    level.allowed_values = Some(vec!["debug".to_owned(), "info".to_owned()]);
    level.quoting = Some(Quoting::ShellQuote);
    level.default_value = Some("info".to_owned());
    let template = UserTemplate {
        platform: UserPlatform::Windows,
        name: String::new(),
        command_pattern: "Get-X {level} {count} {drive}".to_owned(),
        parameters: vec![
            level,
            typed("count", "number"),
            typed("drive", "driveletter"),
        ],
    };
    let mut command = UserLibrary::default();
    let id = command
        .upsert(&Draft {
            title: "Typed".to_owned(),
            category: "Sync".to_owned(),
            platform: UserPlatform::Windows,
            windows_pattern: template.command_pattern.clone(),
            ..Draft::default()
        })
        .expect("made");
    // The parameters the editor leaves to be set, set as a file written by the C# sets them.
    let mut edited = command.get(&id).expect("made").clone();
    edited.windows_command_template = Some(template.clone());
    let sent = file(&[serde_json::to_value(&edited).expect("json")]);
    let mut received = UserLibrary::default();
    let report = received.sync_merge(&sent).expect("received");
    assert_eq!((report.imported, report.skipped), (1, 0), "{report:?}");
    // And once more through the export a push writes.
    let mut again = UserLibrary::default();
    again.sync_merge(&received.export()).expect("again");
    for library in [&received, &again] {
        let kept = library.get(&id).expect("kept");
        let parameters = &kept
            .windows_command_template
            .as_ref()
            .expect("template")
            .parameters;
        assert_eq!(parameters, &template.parameters);
    }
}

// ----- merging the runbooks -----

fn runbook(id: &str, name: &str, command: &str) -> Runbook {
    let mut runbook = Runbook::new(name);
    runbook.public_id = id.to_owned();
    runbook
        .steps
        .push(Step::literal(command, TemplatePlatform::Linux));
    runbook
}

fn no_actions(_: &str) -> Option<&'static crate::tools::user_commands::UserCommand> {
    None
}

const RUNBOOK_A: &str = "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11";
const RUNBOOK_B: &str = "1c1f9a3d-6b2e-4d64-8b65-3e5a4a8d0f22";

fn key(identity: &str) -> String {
    identity.chars().filter(|c| *c != '-').collect()
}

fn values(runbooks: &[Runbook]) -> Vec<Value> {
    runbooks.iter().map(Runbook::to_value).collect()
}

fn store_of(runbooks: &[Runbook]) -> RunbookStore {
    let mut store = RunbookStore::default();
    for runbook in runbooks {
        store.keep(runbook.clone(), &no_actions).expect("kept");
    }
    store
}

#[test]
fn a_new_remote_runbook_is_added_and_agreed_with() {
    let mut store = RunbookStore::default();
    let mut bases = BTreeMap::new();
    let remote = runbook(RUNBOOK_A, "Restart", "systemctl restart x");
    let report = merge_runbooks(
        &mut store,
        &values(&[remote.clone()]),
        &no_actions,
        &mut bases,
    );
    assert_eq!((report.imported, report.updated), (1, 0));
    assert_eq!(store.runbooks().len(), 1);
    assert_eq!(bases.get(&key(RUNBOOK_A)), Some(&digest(&remote)));
}

#[test]
fn a_runbook_the_user_has_not_changed_takes_the_remote_edit() {
    let original = runbook(RUNBOOK_A, "Restart", "systemctl restart x");
    let mut store = store_of(&[original.clone()]);
    let mut bases = bases_of(&store);
    let edited = runbook(RUNBOOK_A, "Restart x", "systemctl restart x --now");
    let report = merge_runbooks(
        &mut store,
        &values(&[edited.clone()]),
        &no_actions,
        &mut bases,
    );
    assert_eq!((report.imported, report.updated), (0, 1));
    assert!(report.kept_local.is_empty());
    assert_eq!(store.get(RUNBOOK_A), Some(&edited));
    assert_eq!(bases.get(&key(RUNBOOK_A)), Some(&digest(&edited)));
}

#[test]
fn a_local_edit_is_kept_and_a_conflict_is_named() {
    let original = runbook(RUNBOOK_A, "Restart", "systemctl restart x");
    let mut bases = bases_of(&store_of(&[original.clone()]));
    // The user edited it here.
    let mine = runbook(RUNBOOK_A, "Restart (mine)", "systemctl restart x");
    let mut store = store_of(&[mine.clone()]);
    // The remote still has the original: a local edit, not a conflict, nothing to say.
    let report = merge_runbooks(
        &mut store,
        &values(&[original.clone()]),
        &no_actions,
        &mut bases,
    );
    assert_eq!(report, RunbookMerge::default());
    assert_eq!(store.get(RUNBOOK_A), Some(&mine));
    // The remote changed it too: kept, and named.
    let theirs = runbook(RUNBOOK_A, "Restart (theirs)", "systemctl restart x");
    let report = merge_runbooks(&mut store, &values(&[theirs]), &no_actions, &mut bases);
    assert_eq!((report.imported, report.updated), (0, 0));
    assert_eq!(report.kept_local, ["Restart (mine)"]);
    assert_eq!(store.get(RUNBOOK_A), Some(&mine));
}

#[test]
fn a_runbook_never_synchronized_that_differs_is_kept_and_named() {
    let mine = runbook(RUNBOOK_A, "Mine", "ls");
    let mut store = store_of(&[mine.clone()]);
    let mut bases = BTreeMap::new();
    let theirs = runbook(RUNBOOK_A, "Theirs", "ls -l");
    let report = merge_runbooks(&mut store, &values(&[theirs]), &no_actions, &mut bases);
    assert_eq!(report.kept_local, ["Mine"]);
    assert_eq!(store.get(RUNBOOK_A), Some(&mine));
}

#[test]
fn a_runbook_that_was_only_run_or_imported_is_not_a_local_edit() {
    // A run writes nothing into a runbook, and an import of the very file the repository
    // holds makes the same runbook: the base still matches, the remote edit is taken.
    let original = runbook(RUNBOOK_A, "Restart", "systemctl restart x");
    let mut store = store_of(&[original.clone()]);
    let mut bases = bases_of(&store);
    // "Imported": the same runbook merged again from a file.
    let again = merge_runbooks(
        &mut store,
        &values(&[original.clone()]),
        &no_actions,
        &mut bases,
    );
    assert_eq!(again, RunbookMerge::default());
    assert_eq!(bases.get(&key(RUNBOOK_A)), Some(&digest(&original)));
    // The remote edits it: the library takes the edit, no conflict.
    let edited = runbook(RUNBOOK_A, "Restart", "systemctl restart y");
    let report = merge_runbooks(
        &mut store,
        &values(&[edited.clone()]),
        &no_actions,
        &mut bases,
    );
    assert_eq!((report.updated, report.kept_local.len()), (1, 0));
    assert_eq!(store.get(RUNBOOK_A), Some(&edited));
}

#[test]
fn a_runbook_that_breaks_a_rule_is_left_out_and_the_others_are_kept() {
    let mut store = RunbookStore::default();
    let mut bases = BTreeMap::new();
    let mut bad = runbook(RUNBOOK_A, "Bad", "ls");
    bad.steps[0] = Step::literal("ls\nrm -rf /", TemplatePlatform::Linux);
    let good = runbook(RUNBOOK_B, "Good", "ls");
    let report = merge_runbooks(&mut store, &values(&[bad, good]), &no_actions, &mut bases);
    assert_eq!((report.imported, report.skipped), (1, 1));
    assert_eq!(
        report.first_skipped.as_ref().map(|(name, _)| name.as_str()),
        Some("Bad")
    );
    assert!(store.get(RUNBOOK_A).is_none() && store.get(RUNBOOK_B).is_some());
}

// ----- the file the repository holds -----

#[test]
fn the_pushed_file_is_the_export_of_the_library_with_its_runbooks() {
    let mut library = UserLibrary::default();
    library
        .sync_merge(&file(&[action("a1", "One", NEW, "ls", "Info")]))
        .expect("seed");
    let commands: Vec<_> = library
        .commands()
        .iter()
        .map(library_transfer::action_value)
        .collect();
    let runbooks = [runbook(RUNBOOK_A, "Restart", "systemctl restart x")];
    let sent = library_transfer::export_text(&commands, &runbooks);
    // What a pull reads is held to the checks of an import first.
    let carried = library_transfer::precheck(&sent).expect("an export");
    assert_eq!(carried.len(), 1);
    let mut received = UserLibrary::default();
    let report = received.sync_merge(&sent).expect("received");
    assert_eq!(report.imported, 1);
    assert_eq!(received.get("a1").expect("a1").title, "One");
}

// ----- the git program, against a real repository -----

fn git_program() -> Option<&'static Path> {
    let program = git::program();
    if program.is_none() {
        eprintln!("skipped: no git on the path");
    }
    program
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

fn bare_remote(program: &Path, parent: &Path) -> String {
    let path = parent.join("remote.git");
    let made = Command::new(program)
        .args(["init", "--bare", "--quiet"])
        .arg(&path)
        .status()
        .expect("git runs");
    assert!(made.success());
    file_url(&path)
}

struct Place {
    settings: SyncSettings,
    state: tempfile::TempDir,
    _parent: tempfile::TempDir,
}

fn place(url: &str, name: &str) -> Place {
    let parent = tempfile::tempdir().expect("parent");
    let state = tempfile::tempdir().expect("state");
    let folder = parent.path().join(name);
    Place {
        settings: settings(url, &folder),
        state,
        _parent: parent,
    }
}

fn pull_from(place: &Place) -> Result<PullOutput, SyncFailure> {
    pull(place.state.path(), &place.settings)
}

fn push_from(place: &Place, text: &str) -> Result<PushOutput, SyncFailure> {
    push(place.state.path(), &place.settings, text)
}

#[test]
fn a_library_goes_through_a_bare_repository_and_back() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(program) = git_program() else {
        return;
    };
    let remote_parent = tempfile::tempdir().expect("remote");
    let url = bare_remote(program, remote_parent.path());
    let first = place(&url, "first clone");
    let second = place(&url, "second clone");

    // Nothing there yet: a pull brings nothing, and does not fail.
    let empty = pull_from(&first).expect("an empty remote");
    assert_eq!(
        empty,
        PullOutput {
            text: None,
            commits: 0
        }
    );

    // The first push creates the branch.
    let one = file(&[action("a1", "One", NEW, "ls", "Info")]);
    assert_eq!(
        push_from(&first, &one).expect("pushed"),
        PushOutput { pushed: true }
    );
    // The same library again sends nothing.
    assert_eq!(
        push_from(&first, &one).expect("again"),
        PushOutput { pushed: false }
    );

    // The second computer pulls it, and edits it.
    let pulled = pull_from(&second).expect("pulled");
    assert_eq!(pulled.text.as_deref(), Some(one.as_str()));
    assert_eq!(pulled.commits, 1);
    let two = file(&[
        action("a1", "One", NEW, "ls", "Info"),
        action("a2", "Two", NEWER, "pwd", "Run"),
    ]);
    assert!(push_from(&second, &two).expect("pushed").pushed);

    // The first, behind, cannot push over it.
    let stale = file(&[action("a9", "Stale", NEWER, "id", "Info")]);
    assert_eq!(push_from(&first, &stale), Err(SyncFailure::PushRejected));
    // It pulls, then pushes.
    let pulled = pull_from(&first).expect("pulled");
    assert_eq!(pulled.text.as_deref(), Some(two.as_str()));
    assert_eq!(pulled.commits, 1);
    let three = file(&[action("a3", "Three", NEWER, "id", "Info")]);
    assert!(push_from(&first, &three).expect("pushed").pushed);
    assert_eq!(
        pull_from(&second).expect("pulled").text.as_deref(),
        Some(three.as_str())
    );
    // Nothing was checked out of what was pulled: the file is read from git's objects, and
    // the folder only ever holds what this computer wrote.
    assert_eq!(
        std::fs::read_to_string(second.settings.local_path.clone() + "/" + REPOSITORY_FILE)
            .expect("the file")
            .contains("Three"),
        true
    );
}

#[test]
fn a_commit_the_remote_never_got_does_not_block_the_next_pull() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(program) = git_program() else {
        return;
    };
    let remote_parent = tempfile::tempdir().expect("remote");
    let url = bare_remote(program, remote_parent.path());
    let first = place(&url, "first");
    let second = place(&url, "second");
    assert!(
        push_from(&first, &file(&[action("a1", "One", NEW, "ls", "Info")]))
            .expect("pushed")
            .pushed
    );
    pull_from(&second).expect("pulled");
    // The second commits locally but its push is lost (the remote moves on meanwhile).
    let moved = file(&[
        action("a1", "One", NEW, "ls", "Info"),
        action("a2", "Two", NEW, "id", "Info"),
    ]);
    assert!(push_from(&first, &moved).expect("pushed").pushed);
    let local_only = &second.settings.local_path;
    std::fs::write(
        Path::new(local_only).join(REPOSITORY_FILE),
        file(&[action("a7", "Local", NEW, "id", "Info")]),
    )
    .expect("written");
    for arguments in [
        vec!["add", "--", REPOSITORY_FILE],
        vec![
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "local",
        ],
    ] {
        let status = Command::new(program)
            .args(&arguments)
            .current_dir(local_only)
            .status()
            .expect("git runs");
        assert!(status.success());
    }
    // Diverged: the pull still brings the remote's file and leaves the folder usable.
    let pulled = pull_from(&second).expect("pulled");
    assert_eq!(pulled.text.as_deref(), Some(moved.as_str()));
    let back = file(&[
        action("a1", "One", NEW, "ls", "Info"),
        action("a2", "Two", NEW, "id", "Info"),
        action("a7", "Local", NEWER, "id", "Info"),
    ]);
    assert!(push_from(&second, &back).expect("pushed").pushed);
}

#[test]
fn a_folder_with_other_files_or_another_remote_is_left_alone() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(program) = git_program() else {
        return;
    };
    let remote_parent = tempfile::tempdir().expect("remote");
    let url = bare_remote(program, remote_parent.path());
    let busy = place(&url, "busy");
    std::fs::create_dir_all(&busy.settings.local_path).expect("folder");
    std::fs::write(
        Path::new(&busy.settings.local_path).join("notes.txt"),
        "mine",
    )
    .expect("file");
    assert_eq!(pull_from(&busy), Err(SyncFailure::FolderNotEmpty));
    assert_eq!(push_from(&busy, "{}"), Err(SyncFailure::FolderNotEmpty));
    assert_eq!(
        std::fs::read_to_string(Path::new(&busy.settings.local_path).join("notes.txt"))
            .expect("kept"),
        "mine"
    );
    // A repository of another remote.
    let other = place(&url, "other");
    push_from(&other, &file(&[action("a1", "One", NEW, "ls", "Info")])).expect("pushed");
    let mut elsewhere = other.settings.clone();
    elsewhere.remote_url = format!("{url}-elsewhere");
    assert_eq!(
        pull(other.state.path(), &elsewhere),
        Err(SyncFailure::OtherRepository)
    );
    // And a branch that is not the one set.
    let mut renamed = other.settings.clone();
    renamed.branch = "release".to_owned();
    assert_eq!(
        pull(other.state.path(), &renamed),
        Err(SyncFailure::OtherBranch)
    );
}

#[test]
fn a_hook_of_the_repository_never_runs() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(program) = git_program() else {
        return;
    };
    let remote_parent = tempfile::tempdir().expect("remote");
    let url = bare_remote(program, remote_parent.path());
    let local = place(&url, "hooked");
    push_from(&local, &file(&[action("a1", "One", NEW, "ls", "Info")])).expect("pushed");
    let marker = remote_parent.path().join("hook-ran");
    let hooks = Path::new(&local.settings.local_path)
        .join(".git")
        .join("hooks");
    std::fs::create_dir_all(&hooks).expect("hooks");
    let script = format!(
        "#!/bin/sh\ntouch '{}'\n",
        marker.to_string_lossy().replace('\\', "/")
    );
    for hook in [
        "pre-commit",
        "commit-msg",
        "pre-push",
        "post-commit",
        "post-merge",
    ] {
        std::fs::write(hooks.join(hook), &script).expect("hook");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hooks.join(hook), std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
    }
    let two = file(&[action("a2", "Two", NEW, "ls", "Info")]);
    assert!(push_from(&local, &two).expect("pushed").pushed);
    pull_from(&local).expect("pulled");
    assert!(!marker.exists(), "a hook ran");
}

#[test]
fn a_folder_named_with_shell_syntax_is_only_a_name() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(program) = git_program() else {
        return;
    };
    let remote_parent = tempfile::tempdir().expect("remote");
    let url = bare_remote(program, remote_parent.path());
    let parent = tempfile::tempdir().expect("parent");
    let state = tempfile::tempdir().expect("state");
    let name = "it's a $(touch pwned) & `x`; folder";
    let folder = parent.path().join(name);
    let set = settings(&url, &folder);
    assert!(
        push(
            state.path(),
            &set,
            &file(&[action("a1", "One", NEW, "ls", "Info")])
        )
        .expect("pushed")
        .pushed
    );
    assert!(!parent.path().join("pwned").exists());
    assert!(folder.join(".git").exists());
}

#[test]
fn a_remote_that_is_not_there_fails_without_hanging() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(_) = git_program() else {
        return;
    };
    let parent = tempfile::tempdir().expect("parent");
    let missing = file_url(&parent.path().join("nothing.git"));
    let local = place(&missing, "nowhere");
    let failure = pull_from(&local).expect_err("no remote");
    assert!(
        matches!(
            failure,
            SyncFailure::RemoteNotFound(_) | SyncFailure::Git(_)
        ),
        "{failure:?}"
    );
}

#[test]
fn a_second_operation_at_once_is_refused() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let held = acquire().expect("free");
    let parent = tempfile::tempdir().expect("parent");
    let local = place("https://example.com/x.git", "busy");
    assert_eq!(pull_from(&local), Err(SyncFailure::Busy));
    assert_eq!(push_from(&local, "{}"), Err(SyncFailure::Busy));
    drop(held);
    drop(parent);
    // Settings are checked before anything is started.
    let mut refused = local.settings.clone();
    refused.remote_url = "ext::sh -c evil".to_owned();
    assert_eq!(
        pull(local.state.path(), &refused),
        Err(SyncFailure::Settings(SettingsIssue::UrlHidden))
    );
}

#[test]
fn a_file_larger_than_a_library_is_not_sent() {
    let _turn = TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let local = place("https://example.com/x.git", "big");
    let big = "x".repeat(usize::try_from(MAX_FILE_BYTES).expect("fits") + 1);
    assert_eq!(push_from(&local, &big), Err(SyncFailure::FileTooLarge));
}
