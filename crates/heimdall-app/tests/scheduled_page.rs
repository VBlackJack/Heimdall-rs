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

//! The Scheduled page and its dialog, as the C# ones: adding, editing and deleting a task,
//! what the dialog refuses, its question before losing what was changed, the sessions it
//! offers, and the list's switch saved at once.

use std::path::Path;

use heimdall_app::scheduled_form::{Fields, ScheduleKind, ScheduledForm, ScheduledProblem};
use heimdall_app::{
    App, AppConfig, Dialog, Message, Notice, ProfileKind, ScheduledMessage, ScheduledNotice,
    SystemCredentials,
};
use heimdall_core::profile::{
    LocalArguments, LocalCommand, LocalProfile, ProfileId, SshMode, SshProfile,
};
use heimdall_core::scheduled::{self, Outcome, Schedule, ScheduledTask, ScheduledTasks};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

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
        ssh_mode: SshMode::Embedded,
        x11_forwarding: false,
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

/// The application over `dir`: SSH profiles "web" and "db", "putty" opened in `PuTTY`, the
/// local program "tool", and `tasks`.
fn app(dir: &Path, tasks: Vec<ScheduledTask>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    let mut putty = ssh("putty", "PuTTY box");
    putty.ssh_mode = SshMode::External;
    store.merge([ssh("web", "web"), ssh("db", "db"), putty]);
    store.merge_local([LocalProfile {
        remote_os: heimdall_core::profile::RemoteOs::default(),
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
    }]);
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

fn saved(dir: &Path) -> Vec<ScheduledTask> {
    ScheduledTasks::load(&scheduled::scheduled_path(&dir.join("profiles.toml")))
        .expect("read")
        .tasks()
        .to_vec()
}

fn page(app: &mut App, message: ScheduledMessage) {
    app.update(Message::Scheduled(message));
}

fn form(app: &App) -> &ScheduledForm {
    match &app.dialog {
        Some(Dialog::ScheduledTask(form)) => form,
        other => panic!("the dialog expected, got {other:?}"),
    }
}

/// The dialog's fields changed as `change` does.
fn fill(app: &mut App, change: impl FnOnce(&mut Fields)) {
    let mut fields = form(app).fields.clone();
    change(&mut fields);
    page(app, ScheduledMessage::Field(fields));
}

fn task(id: &str, enabled: bool) -> ScheduledTask {
    ScheduledTask {
        id: id.to_owned(),
        server_id: Some(ProfileId::new("web")),
        server_name: "web".to_owned(),
        connection_type: "SSH".to_owned(),
        enabled,
        schedule: Schedule::Interval { minutes: 15 },
        last_run: Some(1_000),
        next_run: Some(4_102_444_800),
        last_outcome: Some(Outcome::Ran),
        failures: 2,
    }
}

#[test]
fn the_dialog_offers_only_the_sessions_a_schedule_opens_as_name_and_type() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), Vec::new());
    let mut offered: Vec<(String, ProfileKind)> = app
        .scheduled_choices()
        .into_iter()
        .map(|choice| (choice.name, choice.kind))
        .collect();
    offered.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        offered,
        [
            ("db".to_owned(), ProfileKind::Ssh),
            ("web".to_owned(), ProfileKind::Ssh)
        ],
        "neither PuTTY nor a local program"
    );
}

#[test]
fn a_task_added_in_the_dialog_is_written_timed_and_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Vec::new());
    page(&mut app, ScheduledMessage::Add);
    assert!(!form(&app).dirty());
    app.update(Message::ConfirmDialog);
    assert_eq!(
        form(&app).problem,
        Some(ScheduledProblem::ServerRequired),
        "refused, the dialog stays"
    );
    fill(&mut app, |fields| {
        fields.server = Some(ProfileId::new("db"));
        fields.time = "25:00".to_owned();
    });
    assert_eq!(
        form(&app).problem,
        Some(ScheduledProblem::TimeInvalid),
        "checked again as it changes"
    );
    fill(&mut app, |fields| fields.time = "6:30".to_owned());
    assert_eq!(form(&app).problem, None);
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let tasks = saved(dir.path());
    let [added] = tasks.as_slice() else {
        panic!("{tasks:?}");
    };
    assert_eq!(added.server_id, Some(ProfileId::new("db")));
    assert_eq!(added.server_name, "db");
    assert_eq!(added.connection_type, "SSH");
    assert_eq!(
        added.schedule,
        Schedule::Daily {
            time: "06:30".to_owned()
        }
    );
    assert!(added.enabled);
    assert!(added.next_run.is_some(), "timed");
    assert_eq!(app.scheduled_tasks(), tasks.as_slice());
    assert_eq!(app.selected_scheduled(), Some(added.id.as_str()));
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Added("db".to_owned())))
    );
}

#[test]
fn a_task_edited_keeps_its_identifier_and_its_last_run() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![task("t", true)]);
    page(&mut app, ScheduledMessage::Edit("t".to_owned()));
    assert_eq!(form(&app).editing.as_deref(), Some("t"));
    assert_eq!(form(&app).fields.kind, ScheduleKind::Interval);
    assert_eq!(form(&app).fields.server, Some(ProfileId::new("web")));
    fill(&mut app, |fields| fields.minutes = "0".to_owned());
    app.update(Message::ConfirmDialog);
    assert_eq!(form(&app).problem, Some(ScheduledProblem::IntervalInvalid));
    fill(&mut app, |fields| fields.minutes = "45".to_owned());
    app.update(Message::ConfirmDialog);
    let tasks = saved(dir.path());
    let [edited] = tasks.as_slice() else {
        panic!("{tasks:?}");
    };
    assert_eq!(edited.id, "t");
    assert_eq!(edited.schedule, Schedule::Interval { minutes: 45 });
    assert_eq!(edited.last_run, Some(1_000), "kept");
    assert_eq!(edited.failures, 0, "a task saved again starts afresh");
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Updated(
            "web".to_owned()
        )))
    );
}

#[test]
fn closing_a_changed_dialog_asks_first_and_can_go_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Vec::new());
    page(&mut app, ScheduledMessage::Add);
    app.update(Message::DismissDialog);
    assert!(app.dialog.is_none(), "nothing changed: closed at once");

    page(&mut app, ScheduledMessage::Add);
    fill(&mut app, |fields| fields.enabled = false);
    app.update(Message::DismissDialog);
    assert!(
        matches!(app.dialog, Some(Dialog::ConfirmDiscardScheduled(_))),
        "{:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    assert!(!form(&app).fields.enabled, "back, as it was");
    app.update(Message::DismissDialog);
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "discarded");
    assert!(saved(dir.path()).is_empty(), "nothing written");
}

#[test]
fn a_task_is_deleted_only_once_confirmed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![task("t", true), task("u", true)]);
    page(&mut app, ScheduledMessage::DeleteSelected);
    assert!(app.dialog.is_none(), "nothing selected");
    page(&mut app, ScheduledMessage::Select("t".to_owned()));
    page(&mut app, ScheduledMessage::DeleteSelected);
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDeleteScheduled { id, name }) if id == "t" && name == "web"
    ));
    app.update(Message::DismissDialog);
    assert_eq!(saved(dir.path()).len(), 2, "kept");
    page(&mut app, ScheduledMessage::DeleteSelected);
    app.update(Message::ConfirmDialog);
    let left: Vec<String> = saved(dir.path()).into_iter().map(|task| task.id).collect();
    assert_eq!(left, ["u"]);
    assert_eq!(app.selected_scheduled(), None);
    assert_eq!(
        app.notice(),
        Some(&Notice::Scheduled(ScheduledNotice::Deleted(
            "web".to_owned()
        )))
    );
}

#[test]
fn the_list_switch_is_saved_at_once_and_a_task_turned_on_is_timed_afresh() {
    let dir = tempfile::tempdir().expect("dir");
    let mut stale = task("t", false);
    stale.next_run = Some(1_000);
    stale.failures = scheduled::MAX_CONSECUTIVE_FAILURES;
    let mut app = app(dir.path(), vec![stale]);
    page(&mut app, ScheduledMessage::Toggle("t".to_owned(), true));
    let [on] = saved(dir.path()).try_into().expect("one task");
    assert!(on.enabled, "written at once");
    assert_eq!(on.failures, 0, "its failures forgotten");
    assert!(
        on.next_run.is_some_and(|next| next > 1_000),
        "never due at once: {on:?}"
    );
    page(&mut app, ScheduledMessage::Toggle("t".to_owned(), false));
    let [off] = saved(dir.path()).try_into().expect("one task");
    assert!(!off.enabled);
}

#[test]
fn a_task_whose_session_a_schedule_no_longer_opens_is_edited_without_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut refused = task("t", true);
    refused.server_id = Some(ProfileId::new("putty"));
    let mut app = app(dir.path(), vec![refused]);
    page(&mut app, ScheduledMessage::Edit("t".to_owned()));
    assert_eq!(form(&app).fields.server, None, "to be chosen again");
    app.update(Message::ConfirmDialog);
    assert_eq!(form(&app).problem, Some(ScheduledProblem::ServerRequired));
}
