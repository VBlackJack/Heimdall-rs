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

//! The Scheduled page drawn headless, as the C# one: its place in the navigation, its empty
//! state, its grid, the dialog with its fields, its check and its preview, and the questions
//! before deleting a task or losing what was changed.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, Message as AppMessage, ScheduledMessage};
use heimdall_core::profile::{ProfileId, SshMode, SshProfile};
use heimdall_core::scheduled::{
    self, FailureKind, Outcome, RefusalKind, Schedule, ScheduledTask, ScheduledTasks,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Destination, Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);

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

fn shell(dir: &Path, tasks: Vec<ScheduledTask>) -> Shell {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([ssh("web", "web")]);
    store.save().expect("save");
    let (mut held, _) = ScheduledTasks::open(&scheduled::scheduled_path(&profiles_file));
    held.merge(tasks).expect("tasks written");
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::Navigate(Destination::Scheduled));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

fn scheduled_message(message: ScheduledMessage) -> Message {
    Message::App(AppMessage::Scheduled(message))
}

fn task(outcome: Option<Outcome>, failures: u32, enabled: bool) -> ScheduledTask {
    ScheduledTask {
        id: "t".to_owned(),
        server_id: Some(ProfileId::new("web")),
        server_name: "web".to_owned(),
        connection_type: "SSH".to_owned(),
        enabled,
        schedule: Schedule::Interval { minutes: 15 },
        last_run: None,
        next_run: Some(4_102_444_800),
        last_outcome: outcome,
        failures,
    }
}

#[test]
fn the_scheduled_page_comes_between_tunnels_and_tools_as_the_csharp_toolbar() {
    assert_eq!(
        Destination::ALL,
        [
            Destination::Sessions,
            Destination::Tunnels,
            Destination::Scheduled,
            Destination::Tools,
            Destination::Settings,
            Destination::About,
        ]
    );
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path(), Vec::new());
    let mut ui = simulator(&shell);
    for label in [
        "Sessions",
        "Tunnels",
        "Scheduled",
        "Tools",
        "Settings",
        "About",
    ] {
        ui.find(label).expect(label);
    }
    ui.find("Scheduled Connections").expect("the page shown");
}

#[test]
fn without_a_task_the_page_says_so_and_offers_its_own_add() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), Vec::new());
    {
        let mut ui = simulator(&shell);
        ui.find("No scheduled tasks configured.")
            .expect("the C# empty state");
        ui.find("+ Add").expect("the header's Add");
        ui.click("Create Task").expect("its own Add");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages.iter().any(|message| matches!(
                message,
                Message::App(AppMessage::Scheduled(ScheduledMessage::Add))
            )),
            "opens the dialog"
        );
    }
    let _ = shell.update(scheduled_message(ScheduledMessage::Add));
    let mut ui = simulator(&shell);
    ui.find("Add Scheduled Task").expect("the dialog opened");
    for label in [
        "Session:",
        "Schedule Type:",
        "Time of Day:",
        "24-hour format, e.g. 08:00",
        "Enabled",
        "Save",
        "Cancel",
    ] {
        ui.find(label).expect(label);
    }
}

#[test]
fn a_task_is_listed_with_its_schedule_and_what_became_of_its_last_run() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(
        dir.path(),
        vec![task(
            Some(Outcome::Failed(FailureKind::CredentialUnavailable)),
            0,
            true,
        )],
    );
    let mut ui = simulator(&shell);
    for title in [
        "On",
        "Session",
        "Type",
        "Schedule",
        "Last Run",
        "Next Run",
        "Last Result",
    ] {
        ui.find(title).expect(title);
    }
    ui.find("Every 15 min").expect("the schedule, in words");
    ui.find("no saved credentials for \"web\"")
        .expect("the failure, in words");
}

#[test]
fn a_task_turned_off_after_its_failures_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(
        dir.path(),
        vec![task(
            Some(Outcome::Refused(RefusalKind::ExternalClient)),
            scheduled::MAX_CONSECUTIVE_FAILURES,
            false,
        )],
    );
    let mut ui = simulator(&shell);
    ui.find(
        "Turned off after 3 failures: it opens in an external client (PuTTY or Remote Desktop \
         Connection)",
    )
    .expect("the reason kept");
}

#[test]
fn the_dialog_says_what_is_wrong_and_when_the_task_would_run() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), Vec::new());
    let _ = shell.update(scheduled_message(ScheduledMessage::Add));
    {
        let mut ui = simulator(&shell);
        assert!(
            ui.find("Please select a session.").is_err(),
            "not before a save"
        );
        assert!(
            ui.find("Next execution: today at 08:00").is_ok()
                || ui.find("Next execution: tomorrow at 08:00").is_ok(),
            "the preview of the C# default"
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    {
        let mut ui = simulator(&shell);
        ui.find("Please select a session.")
            .expect("said once refused");
    }
    let mut fields = match &shell.app().dialog {
        Some(heimdall_app::Dialog::ScheduledTask(form)) => form.fields.clone(),
        other => panic!("{other:?}"),
    };
    fields.kind = heimdall_app::scheduled_form::ScheduleKind::Interval;
    fields.minutes = "20".to_owned();
    let _ = shell.update(scheduled_message(ScheduledMessage::Field(fields)));
    let mut ui = simulator(&shell);
    ui.find("Interval (minutes):").expect("the interval asked");
    ui.find("min").expect("its unit");
    ui.find("Next execution: in 20 minutes (every 20 min)")
        .expect("the preview follows");
    assert!(ui.find("Time of Day:").is_err(), "the time hidden");
}

#[test]
fn closing_a_changed_dialog_asks_before_losing_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), Vec::new());
    let _ = shell.update(scheduled_message(ScheduledMessage::Add));
    let mut fields = match &shell.app().dialog {
        Some(heimdall_app::Dialog::ScheduledTask(form)) => form.fields.clone(),
        other => panic!("{other:?}"),
    };
    fields.enabled = false;
    let _ = shell.update(scheduled_message(ScheduledMessage::Field(fields)));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let mut ui = simulator(&shell);
    ui.find("Unsaved Changes").expect("asked");
    ui.find("You have unsaved changes. Discard them and close?")
        .expect("the C# question");
    ui.find("Keep editing").expect("back");
    ui.find("Discard").expect("lose it");
}

#[test]
fn deleting_a_task_is_asked_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), vec![task(None, 0, true)]);
    let _ = shell.update(scheduled_message(ScheduledMessage::Select("t".to_owned())));
    let _ = shell.update(scheduled_message(ScheduledMessage::DeleteSelected));
    let mut ui = simulator(&shell);
    ui.find("Delete Scheduled Task").expect("the title");
    ui.find("Are you sure you want to delete the scheduled task \"web\"?")
        .expect("the C# question");
}

#[test]
fn a_double_click_on_a_row_edits_its_task() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path(), vec![task(None, 0, true)]);
    let messages = common::double_click_messages(|| simulator(&shell), "Every 15 min");
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Scheduled(ScheduledMessage::Edit(id))) if id == "t"
        )),
        "{messages:?}"
    );
}
