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

use super::*;
use heimdall_core::tools::runbooks::{Runbook, RunbookStore, Step};
use heimdall_core::tools::user_commands::{
    Criticality, LIBRARY_FILE, TemplatePlatform, UserLibrary,
};

const OLD: &str = "2026-03-01T00:00:00.0000000Z";
const NEW: &str = "2026-06-01T00:00:00.0000000Z";
const NEWER: &str = "2026-09-01T00:00:00.0000000Z";

const RUNBOOK_ID: &str = "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11";

fn pane(folder: &Path) -> CommandLibraryPane {
    CommandLibraryPane::new(Some(&folder.join("profiles.toml")))
}

fn action(id: &str, title: &str, updated: &str, pattern: &str, level: &str) -> String {
    format!(
        r#"{{
        "id": "{id}", "publicId": "{id}", "title": "{title}", "description": "",
        "category": "Sync", "platform": 1, "level": "{level}", "tags": [],
        "linuxCommandTemplate": {{
            "platform": 1, "name": "", "commandPattern": {pattern:?}, "parameters": []
        }},
        "createdAt": "{OLD}", "updatedAt": "{updated}", "isUserCreated": true
    }}"#
    )
}

fn file(actions: &[String], runbooks: &[Runbook]) -> String {
    let runbooks = if runbooks.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = runbooks
            .iter()
            .map(|each| each.to_value().to_string())
            .collect();
        format!(r#", "runbooks": [{}]"#, listed.join(","))
    };
    format!(
        r#"{{ "schemaVersion": "1.0", "actions": [{}]{runbooks} }}"#,
        actions.join(",")
    )
}

fn runbook(name: &str, command: &str) -> Runbook {
    let mut runbook = Runbook::new(name);
    runbook.public_id = RUNBOOK_ID.to_owned();
    runbook
        .steps
        .push(Step::literal(command, TemplatePlatform::Linux));
    runbook
}

fn pulled(text: &str) -> SyncMessage {
    SyncMessage::Pulled(
        Ok(PullOutput {
            text: Some(text.to_owned()),
            commits: 2,
        }),
        false,
    )
}

fn configure(pane: &mut CommandLibraryPane) {
    for (field, value) in [
        (
            SyncField::Url,
            "https://example.com/team/library.git".to_owned(),
        ),
        (SyncField::AuthorName, "Tester".to_owned()),
        (SyncField::AuthorEmail, "tester@example.com".to_owned()),
    ] {
        pane.sync_message(SyncMessage::Field(field, value));
    }
}

fn done(pane: &CommandLibraryPane) -> (Option<Pulled>, Option<bool>) {
    match &pane.notice {
        Some(Notice::Sync(SyncNotice::Done { pulled, pushed })) => (pulled.clone(), *pushed),
        other => panic!("not a completed synchronization: {other:?}"),
    }
}

#[test]
fn a_pulled_file_is_merged_kept_and_told() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    let text = file(
        &[
            action("a1", "Ping", NEW, "ping {x}", "Info"),
            action("a2", "Restart", NEW, "systemctl restart x", "Run"),
        ],
        &[runbook("Restart it", "systemctl restart x")],
    );
    assert_eq!(pane.sync_message(pulled(&text)), Outcome::None);
    let (pulled, pushed) = done(&pane);
    let pulled = pulled.expect("a pull");
    assert_eq!(pushed, None);
    assert_eq!(pulled.commits, 2);
    assert_eq!((pulled.actions.imported, pulled.actions.skipped), (2, 0));
    assert_eq!(pulled.runbooks.as_ref().map(|each| each.imported), Some(1));
    assert!(pulled.kept_local.is_empty());
    // Kept on disk, with the timestamp of the file.
    let reloaded = UserLibrary::load(&folder.path().join(LIBRARY_FILE)).expect("saved");
    assert_eq!(reloaded.commands().len(), 2);
    assert_eq!(
        reloaded.get("a1").expect("a1").updated_at.as_deref(),
        Some(NEW)
    );
    assert_eq!(reloaded.get("a2").expect("a2").level, Criticality::Run);
    let store =
        RunbookStore::load(&folder.path().join("runbooks.json"), &|_| None).expect("runbooks");
    assert_eq!(store.runbooks().len(), 1);
    // The digest the repository agrees with is remembered.
    let state = SyncState::load(&folder.path().join(SETTINGS_FILE)).expect("state");
    assert_eq!(state.runbook_bases.len(), 1);
}

#[test]
fn an_action_newer_here_is_kept_and_counted_with_its_title() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    pane.sync_message(pulled(&file(
        &[
            action("a1", "Edited here", NEWER, "ping {x}", "Info"),
            action("a2", "Edited here too", NEWER, "ls", "Info"),
            action("a3", "Older here", OLD, "pwd", "Info"),
        ],
        &[],
    )));
    pane.sync_message(pulled(&file(
        &[
            action("a1", "Edited there", NEW, "ping -c1 {x}", "Info"),
            action("a2", "Edited there too", NEW, "ls -l", "Info"),
            action("a3", "Newer there", NEW, "pwd -P", "Info"),
        ],
        &[],
    )));
    let (pulled, _) = done(&pane);
    let pulled = pulled.expect("a pull");
    assert_eq!(pulled.actions.updated, 1);
    assert_eq!(pulled.kept_local, ["Edited here", "Edited here too"]);
    let Some(Notice::Sync(notice)) = &pane.notice else {
        panic!("no notice");
    };
    let (said, failed) = notice_text(notice);
    assert!(!failed);
    assert!(
        said.contains("Edited here") && said.contains("Edited here too"),
        "{said}"
    );
    assert!(said.contains('2'), "{said}");
    assert_eq!(pane.library.get("a1").expect("a1").title, "Edited here");
    assert_eq!(pane.library.get("a3").expect("a3").title, "Newer there");
}

#[test]
fn a_runbook_changed_here_and_there_is_kept_and_named_but_a_remote_edit_alone_is_taken() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    pane.sync_message(pulled(&file(
        &[],
        &[runbook("Restart", "systemctl restart x")],
    )));
    // Only the repository changed it: taken.
    pane.sync_message(pulled(&file(
        &[],
        &[runbook("Restart v2", "systemctl restart x")],
    )));
    let (pulled_once, _) = done(&pane);
    assert_eq!(
        pulled_once.expect("pull").runbooks.map(|each| each.updated),
        Some(1)
    );
    // The user edits it here (a run or an import would have changed nothing).
    let mut store =
        RunbookStore::load(&folder.path().join("runbooks.json"), &|_| None).expect("store");
    store
        .keep(runbook("Restart mine", "systemctl restart x"), &|_| None)
        .expect("kept");
    store
        .save(&folder.path().join("runbooks.json"))
        .expect("saved");
    pane.sync_message(pulled(&file(
        &[],
        &[runbook("Restart v3", "systemctl restart y")],
    )));
    let (pulled_twice, _) = done(&pane);
    let pulled_twice = pulled_twice.expect("pull");
    assert_eq!(pulled_twice.kept_local, ["Restart mine"]);
    assert_eq!(pulled_twice.runbooks.map(|each| each.updated), Some(0));
}

#[test]
fn a_pulled_destructive_command_is_dangerous_whatever_the_file_says() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    pane.sync_message(pulled(&file(
        &[action("a1", "Wipe", NEW, "rm -rf /var/data", "Info")],
        &[],
    )));
    assert_eq!(
        pane.library.get("a1").expect("kept").level,
        Criticality::Dangerous
    );
    // Nothing pulled is run: the command is only kept as text.
    assert!(pane.awaited.is_empty());
}

#[test]
fn a_pulled_file_that_is_not_a_library_changes_nothing() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    pane.sync_message(pulled("[1, 2, 3]"));
    assert_eq!(
        pane.notice,
        Some(Notice::Sync(SyncNotice::File(TransferError::Format)))
    );
    pane.sync_message(pulled(r#"{"schemaVersion": "9.0", "actions": []}"#));
    assert!(matches!(
        pane.notice,
        Some(Notice::Sync(SyncNotice::File(TransferError::Version(_))))
    ));
    assert!(pane.library.commands().is_empty());
    // Commands that break the rules of an untrusted file are left out and counted.
    pane.sync_message(pulled(&file(
        &[
            action("a1", "Two lines", NEW, "ls\nrm -rf /", "Info"),
            action("a2", "Fine", NEW, "ls", "Info"),
        ],
        &[],
    )));
    let (pulled, _) = done(&pane);
    let pulled = pulled.expect("a pull");
    assert_eq!((pulled.actions.imported, pulled.actions.skipped), (1, 1));
    assert!(pane.library.get("a1").is_none());
}

#[test]
fn a_remote_without_a_library_is_not_a_failure() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    pane.sync_message(SyncMessage::Pulled(
        Ok(PullOutput {
            text: None,
            commits: 0,
        }),
        false,
    ));
    let (pulled, _) = done(&pane);
    assert!(pulled.expect("a pull").empty);
}

#[test]
fn the_buttons_refuse_what_the_settings_refuse_and_start_nothing_twice() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    // Nothing set.
    assert_eq!(pane.sync_message(SyncMessage::Pull), Outcome::None);
    assert_eq!(
        pane.notice,
        Some(Notice::Sync(SyncNotice::Settings(
            SettingsIssue::UrlMissing
        )))
    );
    // A transport that runs a command, and an option.
    configure(&mut pane);
    for (url, issue) in [
        ("ext::sh -c evil", SettingsIssue::UrlHidden),
        ("ext::evil", SettingsIssue::UrlScheme),
        ("--upload-pack=evil", SettingsIssue::UrlOption),
        ("http://example.com/x.git", SettingsIssue::UrlScheme),
        ("file:///srv/x.git", SettingsIssue::UrlScheme),
        ("ssh://-oProxyCommand=evil/x", SettingsIssue::UrlOption),
        (
            "https://user:token@example.com/x.git",
            SettingsIssue::UrlCredentials,
        ),
    ] {
        pane.sync_message(SyncMessage::Field(SyncField::Url, url.to_owned()));
        assert_eq!(pane.sync_message(SyncMessage::Sync), Outcome::None);
        assert_eq!(pane.notice, Some(Notice::Sync(SyncNotice::Settings(issue))));
        assert!(!pane.sync.busy);
    }
    pane.sync_message(SyncMessage::Field(
        SyncField::Url,
        "https://example.com/x.git".to_owned(),
    ));
    pane.sync_message(SyncMessage::Field(SyncField::Branch, "--force".to_owned()));
    assert_eq!(pane.sync_message(SyncMessage::Pull), Outcome::None);
    pane.sync_message(SyncMessage::Field(SyncField::Branch, "main".to_owned()));
    // A pull starts, once.
    let Outcome::Sync(SyncJob::Pull {
        settings,
        then_push,
        folder: kept_in,
    }) = pane.sync_message(SyncMessage::Pull)
    else {
        panic!("a pull was expected");
    };
    assert!(!then_push && pane.sync.busy);
    assert_eq!(settings.remote_url, "https://example.com/x.git");
    assert_eq!(kept_in, folder.path());
    assert_eq!(pane.sync_message(SyncMessage::Pull), Outcome::None);
    assert_eq!(pane.sync_message(SyncMessage::Sync), Outcome::None);
    // What runs is what is saved.
    let saved = SyncState::load(&folder.path().join(SETTINGS_FILE)).expect("saved");
    assert_eq!(saved.settings.remote_url, "https://example.com/x.git");
}

#[test]
fn a_sync_pulls_merges_and_then_pushes_the_merged_library() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    configure(&mut pane);
    pane.sync_message(pulled(&file(
        &[action("a1", "Mine", NEW, "ls", "Info")],
        &[runbook("Restart", "systemctl restart x")],
    )));
    let Outcome::Sync(SyncJob::Pull { then_push, .. }) = pane.sync_message(SyncMessage::Sync)
    else {
        panic!("a pull was expected");
    };
    assert!(then_push);
    let remote = file(&[action("a2", "Theirs", NEW, "pwd", "Info")], &[]);
    let Outcome::Sync(SyncJob::Push { text, .. }) = pane.sync_message(SyncMessage::Pulled(
        Ok(PullOutput {
            text: Some(remote),
            commits: 1,
        }),
        true,
    )) else {
        panic!("a push was expected");
    };
    // The push carries the merged library: both actions and the runbook.
    let carried = library_transfer::precheck(&text).expect("an export");
    assert_eq!(carried.len(), 1);
    let mut received = UserLibrary::default();
    assert_eq!(received.sync_merge(&text).expect("merged").imported, 2);
    assert!(pane.sync.busy);
    pane.sync_message(SyncMessage::Pushed(Ok(PushOutput { pushed: true })));
    assert!(!pane.sync.busy);
    let (pulled, pushed) = done(&pane);
    assert_eq!(pushed, Some(true));
    assert_eq!(pulled.expect("a pull").actions.imported, 1);
    // The repository now agrees with the runbook as it is here.
    let state = SyncState::load(&folder.path().join(SETTINGS_FILE)).expect("state");
    assert_eq!(state.runbook_bases.len(), 1);
}

#[test]
fn a_failed_pull_stops_a_sync_and_is_told() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    configure(&mut pane);
    pane.sync.busy = true;
    let outcome = pane.sync_message(SyncMessage::Pulled(
        Err(SyncFailure::Authentication("denied".to_owned())),
        true,
    ));
    assert_eq!(outcome, Outcome::None);
    assert!(!pane.sync.busy);
    let Some(Notice::Sync(notice)) = &pane.notice else {
        panic!("no notice");
    };
    let (said, failed) = notice_text(notice);
    assert!(failed && said.contains("denied"), "{said}");
}

#[test]
fn a_push_of_an_empty_library_is_refused_rather_than_emptying_the_repository() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    configure(&mut pane);
    assert_eq!(pane.sync_message(SyncMessage::Push), Outcome::None);
    assert_eq!(pane.notice, Some(Notice::Sync(SyncNotice::NothingToPush)));
    assert!(!pane.sync.busy);
}

#[test]
fn nothing_is_synchronized_while_the_library_or_the_settings_cannot_be_read() {
    let folder = tempfile::tempdir().expect("folder");
    std::fs::write(folder.path().join(LIBRARY_FILE), "not json").expect("written");
    let mut library = pane(folder.path());
    configure(&mut library);
    assert_eq!(library.sync_message(SyncMessage::Sync), Outcome::None);
    assert_eq!(
        library.notice,
        Some(Notice::Sync(SyncNotice::LibraryUnavailable))
    );
    let other = tempfile::tempdir().expect("folder");
    std::fs::write(other.path().join(SETTINGS_FILE), "not json").expect("written");
    let mut settings = pane(other.path());
    configure(&mut settings);
    assert_eq!(settings.sync_message(SyncMessage::Sync), Outcome::None);
    assert_eq!(
        settings.notice,
        Some(Notice::Sync(SyncNotice::StateUnreadable))
    );
    // And the file is still what it was.
    assert_eq!(
        std::fs::read_to_string(other.path().join(SETTINGS_FILE)).expect("kept"),
        "not json"
    );
}

#[test]
fn the_author_starts_from_the_git_identity_only_while_it_is_the_placeholder() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane(folder.path());
    // The first opening asks Git, once.
    assert_eq!(
        pane.sync_message(SyncMessage::Toggle),
        Outcome::Sync(SyncJob::Identity {
            folder: folder.path().to_path_buf()
        })
    );
    pane.sync_message(SyncMessage::Toggle);
    assert_eq!(pane.sync_message(SyncMessage::Toggle), Outcome::None);
    pane.sync_message(SyncMessage::Identity(Some(GitIdentity {
        name: Some("Julien".to_owned()),
        email: Some("julien@example.com".to_owned()),
    })));
    assert_eq!(pane.sync.state.settings.author_name, "Julien");
    assert_eq!(pane.sync.state.settings.author_email, "julien@example.com");
    // Nothing is written until the person saves or synchronizes.
    assert!(!folder.path().join(SETTINGS_FILE).exists());
    // A name chosen is never replaced.
    pane.sync_message(SyncMessage::Field(
        SyncField::AuthorName,
        "Chosen".to_owned(),
    ));
    pane.sync_message(SyncMessage::Identity(Some(GitIdentity {
        name: Some("Other".to_owned()),
        email: None,
    })));
    assert_eq!(pane.sync.state.settings.author_name, "Chosen");
    // A pane whose author is set does not ask at all.
    let again = tempfile::tempdir().expect("folder");
    let mut set = self::pane(again.path());
    set.sync.state.settings.author_name = "Set".to_owned();
    set.sync.state.settings.author_email = "set@example.com".to_owned();
    assert_eq!(set.sync_message(SyncMessage::Toggle), Outcome::None);
}

#[test]
fn every_message_of_the_synchronization_is_translated() {
    // Every notice has a sentence of its own in the window's language (the keys exist in the
    // three languages: the i18n completeness test checks that).
    let issues = [
        SettingsIssue::UrlMissing,
        SettingsIssue::UrlOption,
        SettingsIssue::UrlHidden,
        SettingsIssue::UrlTooLong,
        SettingsIssue::UrlScheme,
        SettingsIssue::UrlFormat,
        SettingsIssue::UrlCredentials,
        SettingsIssue::BranchMissing,
        SettingsIssue::BranchFormat,
        SettingsIssue::AuthorName,
        SettingsIssue::AuthorEmail,
    ];
    let failures = [
        SyncFailure::Busy,
        SyncFailure::GitMissing,
        SyncFailure::FolderNotOurs,
        SyncFailure::Io("d".to_owned()),
        SyncFailure::TimedOut,
        SyncFailure::Authentication("d".to_owned()),
        SyncFailure::RemoteNotFound("d".to_owned()),
        SyncFailure::Network("d".to_owned()),
        SyncFailure::PushRejected,
        SyncFailure::FileTooLarge,
        SyncFailure::Git("d".to_owned()),
    ];
    let mut notices: Vec<SyncNotice> = vec![
        SyncNotice::Saved,
        SyncNotice::StateUnreadable,
        SyncNotice::StateUnsaved,
        SyncNotice::LibraryUnavailable,
        SyncNotice::LibraryUnsaved,
        SyncNotice::NothingToPush,
        SyncNotice::Done {
            pulled: None,
            pushed: Some(false),
        },
    ];
    notices.extend(issues.into_iter().map(SyncNotice::Settings));
    notices.extend(failures.into_iter().map(SyncNotice::Failed));
    for notice in notices {
        let (said, _) = notice_text(&notice);
        assert!(!said.trim().is_empty(), "{notice:?}");
        // A key that is missing shows as its own name.
        assert!(!said.contains("ui-tool-cmdlib-sync"), "{notice:?}: {said}");
    }
}
