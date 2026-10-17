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

//! The scheduled connections' engine, on a clock the tests give: a task due opens its
//! profile unattended, never asking anything; one missed while Heimdall was closed is skipped;
//! one due while the workspace is locked waits for the unlock, then runs once.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, Phase, QuestionId,
    QuestionKind, ScheduledNotice, SettingsMessage, SystemCredentials, TabId, UiError, VaultStatus,
    open_vault,
};
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile};
use heimdall_core::scheduled::{
    self, FailureKind, FixedOffset, MAX_CONSECUTIVE_FAILURES, Outcome, RefusalKind, Schedule,
    ScheduledTask, ScheduledTasks,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, PublicKey, Secret};
use heimdall_term::GridSize;

const MASTER: &str = "correct horse battery staple";

const HOST_KEY: &str = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

/// The clock of the tests: UTC.
const UTC: FixedOffset = FixedOffset(0);

/// The interval of the tasks, in minutes.
const INTERVAL: u32 = 30;

/// Seconds in a minute.
const MINUTE: i64 = 60;

/// A day ahead of now, in seconds since the epoch: a moment none of the tasks has reached at
/// the start, so the start leaves them alone.
fn tomorrow() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    i64::try_from(now).expect("in range") + 86_400
}

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn ssh(id: &str, name: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
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
    }
}

fn local() -> LocalProfile {
    LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: None,
        command: LocalCommand {
            program: Some("C:\\Windows\\System32\\cmd.exe".to_owned()),
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
            run_as_administrator: false,
        },
        approved: None,
        session_logging: None,
    }
}

/// A task opening `server_id`, named `name`, every [`INTERVAL`] minutes, due at `next`.
fn task(id: &str, server_id: Option<&str>, name: &str, next: i64) -> ScheduledTask {
    ScheduledTask {
        id: id.to_owned(),
        server_id: server_id.map(ProfileId::new),
        server_name: name.to_owned(),
        connection_type: "SSH".to_owned(),
        enabled: true,
        schedule: Schedule::Interval { minutes: INTERVAL },
        last_run: None,
        next_run: Some(next),
        last_outcome: None,
        failures: 0,
    }
}

/// The application over `dir`: SSH profiles `a`, `b` and two named "Twin", the local
/// program "Tool", and `tasks`.
fn app(dir: &Path, tasks: Vec<ScheduledTask>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        ssh("a", "server a"),
        ssh("b", "server b"),
        ssh("t1", "Twin"),
        ssh("t2", "Twin"),
    ]);
    store.merge_local([local()]);
    store.save().expect("save");
    let (mut held, _) = ScheduledTasks::open(&scheduled::scheduled_path(&profiles_file));
    held.merge(tasks).expect("tasks written");
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

fn find<'a>(app: &'a App, id: &str) -> &'a ScheduledTask {
    app.scheduled_tasks()
        .iter()
        .find(|task| task.id == id)
        .expect("the task")
}

/// The task as its file holds it.
fn saved(dir: &Path, id: &str) -> ScheduledTask {
    let path = scheduled::scheduled_path(&dir.join("profiles.toml"));
    ScheduledTasks::load(&path)
        .expect("read")
        .get(id)
        .cloned()
        .expect("the task")
}

/// The tab a single connection effect opened.
fn connected(effects: &[Effect]) -> (TabId, AttemptId) {
    let opened: Vec<(TabId, AttemptId)> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Connect { tab, attempt, .. } => Some((*tab, *attempt)),
            _ => None,
        })
        .collect();
    let [one] = opened.as_slice() else {
        panic!("one connection expected: {effects:?}");
    };
    *one
}

fn event(app: &mut App, (tab, attempt): (TabId, AttemptId), event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn failed(name: &str, failure: FailureKind) -> Notice {
    Notice::Scheduled(ScheduledNotice::Failed {
        name: name.to_owned(),
        failure,
    })
}

#[test]
fn a_due_task_opens_its_profile_and_is_timed_again() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    assert!(app.schedules_connections());
    assert!(app.scheduled_tick_at(due - 1, &UTC).is_empty(), "not yet");

    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Triggered(
            "server a".to_owned()
        )))
    );
    let task = find(&app, "t");
    assert_eq!(task.last_run, Some(due));
    assert_eq!(task.next_run, Some(due + i64::from(INTERVAL) * MINUTE));
    assert!(
        app.scheduled_tick_at(due + MINUTE, &UTC).is_empty(),
        "run once"
    );

    event(
        &mut app,
        opened,
        ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    );
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Ran));
    assert_eq!(saved(dir.path(), "t"), *find(&app, "t"), "written");
}

#[test]
fn a_task_missed_while_heimdall_was_closed_is_skipped_and_timed_from_now() {
    let dir = tempfile::tempdir().expect("dir");
    let mut disabled = task("off", Some("b"), "server b", 1_000);
    disabled.enabled = false;
    let mut app = app(
        dir.path(),
        vec![task("t", Some("a"), "server a", 1_000), disabled],
    );
    let start = tomorrow() - 86_400;
    let task = find(&app, "t");
    assert_eq!(task.last_outcome, Some(Outcome::Missed));
    assert_eq!(task.last_run, None, "it did not run");
    assert!(task.next_run.is_some_and(|next| next >= start), "{task:?}");
    let off = find(&app, "off");
    assert_eq!(off.last_outcome, None, "a disabled task misses nothing");
    assert!(off.next_run.is_some_and(|next| next >= start));
    assert_eq!(saved(dir.path(), "t").last_outcome, Some(Outcome::Missed));
    assert!(app.tabs.is_empty(), "no burst");
    assert!(
        app.scheduled_tick_at(start, &UTC).is_empty(),
        "nothing due right after the start"
    );
}

#[test]
fn a_disabled_task_never_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut off = task("t", Some("a"), "server a", due);
    off.enabled = false;
    let mut app = app(dir.path(), vec![off]);
    assert!(!app.schedules_connections(), "no tick needed");
    assert!(app.scheduled_tick_at(due + MINUTE, &UTC).is_empty());
    assert_eq!(find(&app, "t").last_run, None);
}

#[test]
fn a_question_is_declined_and_fails_the_task_quietly() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    let effects = event(
        &mut app,
        opened,
        ConnectionEvent::Question {
            question: QuestionId::fresh(),
            kind: QuestionKind::Password(PasswordQuestion {
                host: "a.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::Answer { answer: None, .. }]),
        "declined: {effects:?}"
    );
    assert!(
        app.tab(opened.0).expect("tab").prompts.is_empty(),
        "never asked"
    );
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Failed(FailureKind::CredentialUnavailable))
    );
    assert_eq!(
        app.notice(),
        Some(&failed("server a", FailureKind::CredentialUnavailable))
    );

    // The failure that follows is the attempt's own: the outcome stays, the tab goes.
    event(
        &mut app,
        opened,
        ConnectionEvent::Failed(UiError::Cancelled),
    );
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Failed(FailureKind::CredentialUnavailable))
    );
    assert!(
        app.tab(opened.0).is_none(),
        "a tab that failed to connect closes"
    );
}

#[test]
fn an_unknown_server_key_is_never_asked_about() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    let effects = event(
        &mut app,
        opened,
        ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:x".to_owned(),
            key: Arc::new(PublicKey::from_openssh(HOST_KEY.trim()).expect("key")),
        },
    );
    assert!(effects.is_empty(), "{effects:?}");
    assert!(
        app.tab(opened.0).is_none(),
        "stopped and closed, not waiting for an answer"
    );
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Failed(FailureKind::ConfirmationRequired))
    );
}

#[test]
fn a_connection_that_fails_is_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    event(&mut app, opened, ConnectionEvent::Failed(UiError::Timeout));
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Failed(FailureKind::ConnectionFailed))
    );
    assert!(app.tab(opened.0).is_none(), "closed");
    assert_eq!(
        app.notice(),
        Some(&failed("server a", FailureKind::ConnectionFailed))
    );
}

#[test]
fn a_task_whose_profile_is_not_found_connects_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(
        dir.path(),
        vec![
            // Its profile gone: the one left with its name is not taken for it.
            task("gone", Some("deleted"), "server a", due),
            task("twin", None, "twin", due),
            task("nobody", None, "Nobody", due),
        ],
    );
    assert!(app.scheduled_tick_at(due, &UTC).is_empty());
    assert!(app.tabs.is_empty());
    for (id, failure) in [
        ("gone", FailureKind::ProfileNotFound),
        ("twin", FailureKind::NameAmbiguous),
        ("nobody", FailureKind::NameNotFound),
    ] {
        let task = find(&app, id);
        assert_eq!(task.last_outcome, Some(Outcome::Failed(failure)), "{id}");
        assert_eq!(task.last_run, Some(due), "{id}");
    }
}

#[test]
fn a_task_with_no_identifier_runs_the_one_profile_of_its_name() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", None, "SERVER B", due)]);
    let effects = app.scheduled_tick_at(due, &UTC);
    let (tab, _) = connected(&effects);
    assert_eq!(app.tab(tab).expect("tab").profile.name(), "server b");
}

#[test]
fn a_local_program_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("tool"), "Tool", due)]);
    let effects = app.scheduled_tick_at(due, &UTC);
    assert!(effects.is_empty(), "nothing started: {effects:?}");
    assert!(app.tabs.is_empty());
    assert!(app.dialog.is_none(), "nothing asked: {:?}", app.dialog);
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Refused(RefusalKind::LocalProgram))
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Refused {
            name: "Tool".to_owned(),
            refusal: RefusalKind::LocalProgram
        }))
    );
}

#[test]
fn windows_hello_is_never_asked_by_a_schedule() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    let effects = app.scheduled_tick_at(due, &UTC);
    assert!(effects.is_empty(), "no prompt: {effects:?}");
    assert!(app.tabs.is_empty());
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Failed(FailureKind::VerificationRequired))
    );
}

#[test]
fn a_task_due_while_a_dialog_is_open_runs_once_it_is_closed() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::ShowVault);
    assert!(app.dialog.is_some());
    assert!(app.scheduled_tick_at(due, &UTC).is_empty(), "held");
    assert!(app.schedules_connections());
    assert_eq!(find(&app, "t").last_outcome, None, "nothing to say yet");
    app.update(Message::DismissDialog);
    connected(&app.scheduled_tick_at(due + MINUTE, &UTC));
    assert!(
        app.scheduled_tick_at(due + 2 * MINUTE, &UTC).is_empty(),
        "once"
    );
}

/// Types `master` into the vault dialog shown and runs its job, as the UI would; what the
/// result asked for.
async fn submit(app: &mut App, master: &str, confirm: bool) -> Vec<Effect> {
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: confirm.then(|| Secret::new(master.to_owned())),
    });
    let (ticket, result) = match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                    ticket,
                },
            ],
        ) => (ticket, open_vault(path, password, job).await),
        other => panic!("expected OpenVault, got {other:?}"),
    };
    app.update(Message::VaultOpened(ticket, result))
}

#[tokio::test]
async fn a_task_due_while_locked_waits_for_the_unlock_then_runs_once() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::ShowVault);
    submit(&mut app, MASTER, true).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);

    app.update(Message::LockVault);
    assert!(app.is_locked());
    assert!(app.scheduled_tick_at(due, &UTC).is_empty(), "deferred");
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Deferred));
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Deferred(
            "server a".to_owned()
        )))
    );
    assert!(
        app.scheduled_tick_at(due + MINUTE, &UTC).is_empty(),
        "still waiting"
    );

    let effects = submit(&mut app, MASTER, false).await;
    assert!(!app.is_locked(), "{:?}", app.dialog);
    connected(&effects);
    assert!(
        app.scheduled_tick_at(due + 2 * MINUTE, &UTC).is_empty(),
        "run once"
    );
}

/// The application with profile `a` replaced by `changed`, and a task opening it.
fn with_a(dir: &Path, changed: SshProfile, due: i64) -> App {
    drop(app(dir, vec![task("t", Some("a"), "server a", due)]));
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([changed]);
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

fn password_question() -> ConnectionEvent {
    ConnectionEvent::Question {
        question: QuestionId::fresh(),
        kind: QuestionKind::Password(PasswordQuestion {
            host: "a.lab".to_owned(),
            port: 22,
            username: "admin".to_owned(),
            attempt: 1,
        }),
    }
}

fn up() -> ConnectionEvent {
    ConnectionEvent::Connected {
        input: Arc::new(NullSink),
    }
}

#[test]
fn a_profile_that_would_start_a_program_or_ask_is_refused_before_anything_opens() {
    let mut putty = ssh("a", "server a");
    putty.ssh_mode = heimdall_core::profile::SshMode::External;
    let mut x11 = ssh("a", "server a");
    x11.x11_forwarding = true;
    let mut steps = ssh("a", "server a");
    steps.post_connect.steps = vec![heimdall_core::post_connect::PostConnectStep::new("uptime")];
    for (profile, refusal) in [
        (putty, RefusalKind::ExternalClient),
        (x11, RefusalKind::X11Forwarding),
        (steps, RefusalKind::StepsNotApproved),
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let due = tomorrow();
        let mut app = with_a(dir.path(), profile, due);
        let effects = app.scheduled_tick_at(due, &UTC);
        assert!(
            effects.is_empty(),
            "{refusal:?}: nothing probed nor started: {effects:?}"
        );
        assert!(app.tabs.is_empty(), "{refusal:?}");
        assert!(app.dialog.is_none(), "{refusal:?}: {:?}", app.dialog);
        assert_eq!(
            find(&app, "t").last_outcome,
            Some(Outcome::Refused(refusal))
        );
    }
}

#[test]
fn a_scheduled_tab_opens_behind_the_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let shown = connected(&app.update(Message::OpenProfile(ProfileId::new("b"))));
    assert_eq!(app.active, Some(shown.0));
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    assert_ne!(opened.0, shown.0);
    assert_eq!(app.active, Some(shown.0), "the keyboard stays where it was");
    assert!(app.is_scheduled_tab(opened.0));
    assert!(!app.is_scheduled_tab(shown.0));
}

#[test]
fn a_task_whose_session_is_still_open_is_skipped() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    let next = due + i64::from(INTERVAL) * MINUTE;
    assert!(
        app.scheduled_tick_at(next, &UTC).is_empty(),
        "still connecting"
    );
    event(&mut app, opened, up());
    let later = next + i64::from(INTERVAL) * MINUTE;
    let effects = app.scheduled_tick_at(later, &UTC);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Connect { .. })),
        "still connected: {effects:?}"
    );
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Ran), "kept");
    assert_eq!(find(&app, "t").last_run, Some(due), "not run again");
}

#[test]
fn a_scheduled_tab_never_reconnects_by_itself() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::Settings(SettingsMessage::SshAutoReconnect(true)));
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    event(&mut app, opened, up());
    let effects = event(
        &mut app,
        opened,
        ConnectionEvent::Failed(UiError::ConnectionLost),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::RetryAt { .. })),
        "no attempt nobody watches: {effects:?}"
    );
    let tab = app
        .tab(opened.0)
        .expect("a dropped session stays to be seen");
    assert!(tab.retry.is_none());
    assert!(matches!(tab.phase, Phase::Failed(_)));
    assert!(tab.prompts.is_empty());
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Ran), "it ran");
}

#[test]
fn the_sftp_pane_docked_beside_a_scheduled_shell_asks_nothing_either() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    let pane = connected(&event(&mut app, opened, up()));
    assert!(
        app.is_scheduled_tab(pane.0),
        "the companion keeps the origin"
    );
    let effects = event(&mut app, pane, password_question());
    assert!(
        matches!(effects.as_slice(), [Effect::Answer { answer: None, .. }]),
        "declined: {effects:?}"
    );
    assert!(app.tab(pane.0).expect("pane").prompts.is_empty());
    assert_eq!(
        find(&app, "t").last_outcome,
        Some(Outcome::Ran),
        "the pane's outcome is not the task's"
    );
}

#[test]
fn a_task_failing_three_times_in_a_row_is_turned_off() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(
        dir.path(),
        vec![task("t", Some("deleted"), "server a", due)],
    );
    let step = i64::from(INTERVAL) * MINUTE;
    let mut at = due;
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert!(app.scheduled_tick_at(at, &UTC).is_empty());
        at += step;
    }
    let task = find(&app, "t");
    assert!(task.suspended(), "{task:?}");
    assert_eq!(
        task.last_outcome,
        Some(Outcome::Failed(FailureKind::ProfileNotFound))
    );
    assert!(saved(dir.path(), "t").suspended(), "written");
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Suspended {
            name: "server a".to_owned(),
            failures: MAX_CONSECUTIVE_FAILURES
        }))
    );
    assert!(!app.schedules_connections());
    assert!(app.scheduled_tick_at(at, &UTC).is_empty(), "off");
    assert_eq!(find(&app, "t").failures, MAX_CONSECUTIVE_FAILURES);
}

#[test]
fn past_the_sessions_threshold_a_task_still_runs_and_the_banner_says_so_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::Settings(SettingsMessage::MaxSessions(1)));
    connected(&app.update(Message::OpenProfile(ProfileId::new("b"))));
    // The C# task goes through the standard connect, which warns and refuses nothing.
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(
        app.session_load_notice(),
        Some(heimdall_app::SessionLoadNotice {
            open: 2,
            threshold: 1
        })
    );
    event(
        &mut app,
        opened,
        ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    );
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Ran));
}

#[tokio::test]
async fn a_lock_while_a_scheduled_tab_connects_defers_the_task() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    app.update(Message::ShowVault);
    submit(&mut app, MASTER, true).await;
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    app.update(Message::LockVault);
    let effects = event(&mut app, opened, password_question());
    assert!(
        matches!(effects.as_slice(), [Effect::Answer { answer: None, .. }]),
        "{effects:?}"
    );
    assert_eq!(find(&app, "t").last_outcome, Some(Outcome::Deferred));
    event(
        &mut app,
        opened,
        ConnectionEvent::Failed(UiError::Cancelled),
    );
    assert!(app.tab(opened.0).is_none(), "closed");
    let effects = submit(&mut app, MASTER, false).await;
    connected(&effects);
}

#[test]
fn a_task_on_a_server_that_ends_its_sessions_keeps_one_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut every_minute = task("t", Some("a"), "server a", due);
    every_minute.schedule = Schedule::Interval { minutes: 1 };
    let mut app = app(dir.path(), vec![every_minute]);
    app.update(Message::Settings(SettingsMessage::SftpBrowser(
        heimdall_core::settings::SftpBrowser {
            auto_open_on_ssh: false,
            ..heimdall_core::settings::SftpBrowser::default()
        },
    )));
    let mut at = due;
    for ending in [
        ConnectionEvent::Closed { exit_status: None },
        ConnectionEvent::Failed(UiError::ConnectionLost),
        ConnectionEvent::Closed {
            exit_status: Some(0),
        },
    ] {
        let opened = connected(&app.scheduled_tick_at(at, &UTC));
        assert_eq!(app.tabs.len(), 1, "the one before closed first");
        event(&mut app, opened, up());
        event(&mut app, opened, ending);
        assert!(
            app.tab(opened.0).is_some(),
            "left to be seen until the next run"
        );
        at += MINUTE;
    }
    let task = find(&app, "t");
    assert_eq!(task.last_outcome, Some(Outcome::Ran));
    assert_eq!(
        task.failures, 0,
        "a session that ended after connecting is no failure"
    );
    assert!(task.enabled);
}

#[test]
fn a_failed_scheduled_tab_closes_alone_never_the_panes_put_beside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let due = tomorrow();
    let mut app = app(dir.path(), vec![task("t", Some("a"), "server a", due)]);
    let mine = connected(&app.update(Message::OpenProfile(ProfileId::new("b"))));
    let opened = connected(&app.scheduled_tick_at(due, &UTC));
    app.update(Message::Split(SplitMessage::Merge {
        host: opened.0,
        tab: mine.0,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    assert_eq!(app.panes_of(opened.0), [opened.0, mine.0]);
    event(&mut app, opened, ConnectionEvent::Failed(UiError::Timeout));
    assert!(app.tab(opened.0).is_none(), "the scheduled pane closed");
    assert!(app.tab(mine.0).is_some(), "the user's pane stays");
}
