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

//! The scheduled tasks' tests: their timing, after the C# `ComputeNextRun`; which profile
//! they run against, after the C# `ScheduledTaskServerResolverTests`; and their file.

use super::*;
use crate::tools::date_time::days_from_civil;

/// Midnight UTC of a date, in seconds since the epoch.
fn midnight(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, month, day) * DAY_SECONDS
}

fn daily(time: &str) -> Schedule {
    Schedule::Daily {
        time: time.to_owned(),
    }
}

const UTC: FixedOffset = FixedOffset(0);

#[test]
fn a_daily_time_still_ahead_runs_today_and_one_past_runs_tomorrow() {
    let day = midnight(2026, 10, 11);
    let now = day + 7 * HOUR_SECONDS;
    assert_eq!(next_run(&daily("08:00"), now, &UTC), day + 8 * HOUR_SECONDS);
    assert_eq!(
        next_run(&daily("06:59"), now, &UTC),
        day + DAY_SECONDS + 6 * HOUR_SECONDS + 59 * MINUTE_SECONDS,
        "past: tomorrow"
    );
    assert_eq!(
        next_run(&daily("07:00"), now, &UTC),
        day + DAY_SECONDS + 7 * HOUR_SECONDS,
        "now: tomorrow"
    );
}

#[test]
fn a_daily_time_within_a_minute_of_now_runs_tomorrow() {
    let day = midnight(2026, 10, 11);
    let now = day + 7 * HOUR_SECONDS + 59 * MINUTE_SECONDS;
    assert_eq!(
        next_run(&daily("08:00"), now, &UTC),
        day + DAY_SECONDS + 8 * HOUR_SECONDS,
        "a minute away is not far enough"
    );
    assert_eq!(
        next_run(&daily("08:00"), now - 1, &UTC),
        day + 8 * HOUR_SECONDS,
        "a minute and a second away runs today"
    );
}

#[test]
fn a_time_that_does_not_read_runs_tomorrow_at_eight() {
    let day = midnight(2026, 10, 11);
    let now = day + HOUR_SECONDS;
    for unread in ["", "8", "24:00", "08:60", "8h00", "08:00:00:00", "1.08:00"] {
        assert_eq!(
            next_run(&daily(unread), now, &UTC),
            day + DAY_SECONDS + 8 * HOUR_SECONDS,
            "{unread:?}: tomorrow, although 08:00 today is still ahead, as the C#"
        );
        assert_eq!(
            daily(unread).time_of_day(),
            Some(TimeOfDay::parse("08:00").expect("time"))
        );
    }
}

#[test]
fn an_interval_runs_that_many_minutes_from_now_and_none_is_an_hour() {
    let now = midnight(2026, 10, 11) + 12_345;
    assert_eq!(
        next_run(&Schedule::Interval { minutes: 30 }, now, &UTC),
        now + 30 * MINUTE_SECONDS
    );
    assert_eq!(
        next_run(&Schedule::Interval { minutes: 0 }, now, &UTC),
        now + i64::from(FALLBACK_INTERVAL_MINUTES) * MINUTE_SECONDS
    );
    assert_eq!(
        Schedule::Interval { minutes: 0 }.interval_minutes(),
        Some(60)
    );
    assert_eq!(daily("08:00").interval_minutes(), None);
    assert_eq!(Schedule::Interval { minutes: 5 }.time_of_day(), None);
}

#[test]
fn a_daily_time_is_on_the_local_wall_clock() {
    // Two hours east of UTC: 08:00 on the wall is 06:00 UTC, and the day turns at 22:00 UTC.
    let east = FixedOffset(2 * 3_600);
    let day = midnight(2026, 10, 11);
    assert_eq!(
        next_run(&daily("08:00"), day + 5 * HOUR_SECONDS, &east),
        day + 6 * HOUR_SECONDS
    );
    assert_eq!(
        next_run(&daily("08:00"), day + 23 * HOUR_SECONDS, &east),
        day + DAY_SECONDS + 6 * HOUR_SECONDS,
        "already the next day on the wall: today there is tomorrow in UTC"
    );
    // Five hours west: 08:00 on the wall is 13:00 UTC; at 02:00 UTC the wall still shows
    // the day before.
    let west = FixedOffset(-5 * 3_600);
    assert_eq!(
        next_run(&daily("08:00"), day + 2 * HOUR_SECONDS, &west),
        day + 13 * HOUR_SECONDS
    );
}

/// A clock that changes once, at `at`, from `before` to `after` seconds east of UTC.
struct Changing {
    at: i64,
    before: i32,
    after: i32,
}

impl LocalClock for Changing {
    fn offset_at(&self, unix_seconds: i64) -> i32 {
        if unix_seconds < self.at {
            self.before
        } else {
            self.after
        }
    }

    fn offset_of_wall_clock(&self, wall_seconds: i64) -> i32 {
        // Before the change, in the hour it skips, and in the hour it repeats: the offset
        // before, as the application's clock answers.
        if wall_seconds < self.at + i64::from(self.before.max(self.after)) {
            self.before
        } else {
            self.after
        }
    }
}

#[test]
fn a_daily_task_keeps_its_wall_clock_time_across_a_change_of_the_clock() {
    // Central Europe goes to summer time on 2026-03-29 at 01:00 UTC.
    let spring = Changing {
        at: midnight(2026, 3, 29) + HOUR_SECONDS,
        before: 3_600,
        after: 7_200,
    };
    let saturday_noon = midnight(2026, 3, 28) + 12 * HOUR_SECONDS;
    assert_eq!(
        next_run(&daily("08:00"), saturday_noon, &spring),
        midnight(2026, 3, 29) + 6 * HOUR_SECONDS,
        "08:00 summer time, not 08:00 winter time"
    );
    assert_eq!(
        next_run(&daily("02:30"), saturday_noon, &spring),
        midnight(2026, 3, 29) + HOUR_SECONDS + 30 * MINUTE_SECONDS,
        "a time the clock skips runs when the wall shows 03:30"
    );
    // And back to winter time on 2026-10-25 at 01:00 UTC.
    let autumn = Changing {
        at: midnight(2026, 10, 25) + HOUR_SECONDS,
        before: 7_200,
        after: 3_600,
    };
    let saturday_noon = midnight(2026, 10, 24) + 12 * HOUR_SECONDS;
    assert_eq!(
        next_run(&daily("08:00"), saturday_noon, &autumn),
        midnight(2026, 10, 25) + 7 * HOUR_SECONDS,
        "08:00 winter time"
    );
    assert_eq!(
        next_run(&daily("02:30"), saturday_noon, &autumn),
        midnight(2026, 10, 25) + 30 * MINUTE_SECONDS,
        "a time the wall shows twice runs the first time"
    );
}

#[test]
fn a_time_of_day_reads_as_the_csharp_writes_it() {
    let read = |text: &str| TimeOfDay::parse(text).map(TimeOfDay::text);
    assert_eq!(read("08:00").as_deref(), Some("08:00"));
    assert_eq!(read("8:05").as_deref(), Some("08:05"));
    assert_eq!(read(" 23:59 ").as_deref(), Some("23:59"));
    assert_eq!(
        read("00:00:30").as_deref(),
        Some("00:00"),
        "seconds ignored"
    );
    for refused in [
        "", "8", "24:00", "12:60", "123:00", "12:5", "12:5a", "-1:00", "12:00:60", "+8:00",
    ] {
        assert_eq!(read(refused), None, "{refused:?}");
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Server {
    id: &'static str,
    name: &'static str,
}

const fn server(id: &'static str, name: &'static str) -> Server {
    Server { id, name }
}

fn resolved<'a>(
    task_id: Option<&str>,
    task_name: &str,
    servers: &'a [Server],
) -> Result<&'a Server, Unresolved> {
    resolve(task_id, task_name, servers, |s| s.id, |s| s.name)
}

fn resolved_id(task_id: Option<&str>, task_name: &str, servers: &[Server]) -> Option<&'static str> {
    resolved(task_id, task_name, servers).ok().map(|s| s.id)
}

#[test]
fn a_task_is_answered_by_the_identifier_it_recorded() {
    let servers = [server("id-b", "Production"), server("id-a", "Production")];
    assert_eq!(
        resolved_id(Some("id-a"), "Production", &servers),
        Some("id-a")
    );
}

#[test]
fn a_task_whose_identifier_is_gone_and_whose_name_is_ambiguous_connects_to_nothing() {
    let servers = [
        server("id-other", "Production"),
        server("id-another", "Production"),
    ];
    assert_eq!(resolved_id(Some("id-gone"), "Production", &servers), None);
}

#[test]
fn deleting_the_profile_a_task_names_does_not_transfer_it_to_the_surviving_homonym() {
    // The name is now unique, and it names the other machine.
    let servers = [server("id-B", "Production")];
    assert_eq!(resolved_id(Some("id-A"), "Production", &servers), None);
}

#[test]
fn a_task_whose_profile_was_recreated_under_a_new_identifier_stops_running() {
    let servers = [server("id-new", "PROD-DC01")];
    assert_eq!(resolved_id(Some("id-old"), "PROD-DC01", &servers), None);
}

#[test]
fn the_same_inventory_still_answers_a_task_whose_identifier_resolves() {
    let servers = [
        server("id-other", "Production"),
        server("id-another", "Production"),
    ];
    assert_eq!(
        resolved_id(Some("id-another"), "Production", &servers),
        Some("id-another")
    );
}

#[test]
fn a_task_with_no_identifier_still_falls_back_to_the_display_name() {
    let servers = [server("id-a", "Lab"), server("id-b", "Production")];
    for task_id in [None, Some("")] {
        assert_eq!(
            resolved_id(task_id, "production", &servers),
            Some("id-b"),
            "{task_id:?}"
        );
    }
}

#[test]
fn a_task_with_no_identifier_and_an_ambiguous_name_connects_to_nothing() {
    let servers = [server("id-a", "Production"), server("id-b", "Production")];
    assert_eq!(resolved_id(None, "Production", &servers), None);
}

#[test]
fn refusing_because_the_identifier_is_gone_is_not_refusing_because_the_name_is_ambiguous() {
    assert_eq!(
        resolved(
            Some("id-gone"),
            "Production",
            &[server("id-b", "Production")]
        ),
        Err(Unresolved::IdentifierNotFound)
    );
    assert_eq!(
        resolved(
            None,
            "Production",
            &[server("id-a", "Production"), server("id-b", "Production")]
        ),
        Err(Unresolved::NameAmbiguous)
    );
    assert_eq!(
        resolved(None, "Production", &[server("id-a", "Lab")]),
        Err(Unresolved::NameNotFound)
    );
    assert_eq!(
        resolved(Some("id-a"), "Production", &[server("id-a", "Production")]),
        Ok(&server("id-a", "Production"))
    );
    assert_eq!(
        Unresolved::IdentifierNotFound.failure(),
        FailureKind::ProfileNotFound
    );
}

#[test]
fn an_identifier_is_matched_exactly_and_does_not_resolve_by_case() {
    let servers = [server("id-a", "Production"), server("id-b", "Production")];
    assert_eq!(resolved_id(Some("ID-A"), "Production", &servers), None);
}

#[test]
fn a_name_is_compared_without_case_beyond_ascii() {
    let servers = [server("id-a", "Serveur Été")];
    assert_eq!(resolved_id(None, "SERVEUR ÉTÉ", &servers), Some("id-a"));
}

fn task(id: &str, outcome: Option<Outcome>) -> ScheduledTask {
    ScheduledTask {
        id: id.to_owned(),
        server_id: Some(ProfileId::new("profile-1")),
        server_name: "web".to_owned(),
        connection_type: "SSH".to_owned(),
        enabled: true,
        schedule: daily("08:00"),
        last_run: Some(1_790_000_000),
        next_run: Some(1_790_086_400),
        last_outcome: outcome,
        failures: 0,
    }
}

fn path(dir: &tempfile::TempDir) -> PathBuf {
    scheduled_path(&dir.path().join("profiles.toml"))
}

#[test]
fn tasks_are_written_and_read_back_each_outcome_included() {
    let dir = tempfile::tempdir().expect("dir");
    let path = path(&dir);
    let (mut tasks, error) = ScheduledTasks::open(&path);
    assert!(error.is_none(), "no file: no task, nothing wrong");
    assert!(tasks.tasks().is_empty());
    let outcomes = [
        None,
        Some(Outcome::Ran),
        Some(Outcome::Missed),
        Some(Outcome::Failed(FailureKind::CredentialUnavailable)),
        Some(Outcome::Deferred),
        Some(Outcome::Refused(RefusalKind::X11Forwarding)),
    ];
    for (index, outcome) in outcomes.into_iter().enumerate() {
        tasks
            .upsert(task(&index.to_string(), outcome))
            .expect("written");
    }
    let mut interval = task("interval", None);
    interval.schedule = Schedule::Interval { minutes: 15 };
    interval.server_id = None;
    interval.last_run = None;
    tasks.upsert(interval).expect("written");
    let read = ScheduledTasks::load(&path).expect("read");
    assert_eq!(read, tasks);
    let text = fs::read_to_string(&path).expect("text");
    assert!(text.contains("version = 1"), "{text}");
    assert!(text.contains("\"credential_unavailable\""), "{text}");
}

#[test]
fn a_task_is_replaced_by_its_identifier_and_removed_by_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = path(&dir);
    let (mut tasks, _) = ScheduledTasks::open(&path);
    tasks.upsert(task("a", None)).expect("written");
    tasks.upsert(task("b", None)).expect("written");
    let mut changed = task("a", Some(Outcome::Ran));
    changed.enabled = false;
    tasks.upsert(changed.clone()).expect("written");
    assert_eq!(tasks.tasks().len(), 2);
    assert_eq!(tasks.get("a"), Some(&changed), "in its place");
    assert_eq!(tasks.tasks()[0].id, "a");
    assert!(tasks.remove("a").expect("written"));
    assert!(!tasks.remove("a").expect("nothing to write"));
    assert_eq!(ScheduledTasks::load(&path).expect("read").tasks().len(), 1);
    let count = tasks
        .merge([task("b", Some(Outcome::Missed)), task("c", None)])
        .expect("written");
    assert_eq!(
        count,
        MergeCount {
            added: 1,
            updated: 1
        }
    );
    assert_eq!(ScheduledTasks::load(&path).expect("read"), tasks);
}

#[test]
fn a_file_that_does_not_read_is_never_written_over() {
    let dir = tempfile::tempdir().expect("dir");
    let path = path(&dir);
    for (text, newer) in [
        ("not = [toml", false),
        ("version = 2\n\n[[task]]\nid = \"a\"\n", true),
    ] {
        fs::write(&path, text).expect("written");
        let (mut tasks, error) = ScheduledTasks::open(&path);
        match error {
            Some(StoreError::UnsupportedVersion {
                found: 2,
                expected: SCHEDULED_VERSION,
                ..
            }) => assert!(newer),
            Some(StoreError::Parse { .. }) => assert!(!newer),
            other => panic!("{other:?}"),
        }
        assert!(tasks.tasks().is_empty());
        assert_eq!(tasks.path(), None, "kept for the run only");
        tasks.upsert(task("a", None)).expect("kept nowhere");
        assert_eq!(tasks.tasks().len(), 1);
        assert_eq!(fs::read_to_string(&path).expect("text"), text, "untouched");
    }
}

#[test]
fn a_new_identifier_is_random_hexadecimal() {
    let first = new_task_id().expect("random");
    let second = new_task_id().expect("random");
    assert_eq!(first.len(), 2 * ID_BYTES);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(first, second);
}

fn ssh_profile() -> SshProfile {
    SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
        vault_entry: None,
        forwards: crate::profile::Forwards::default(),
        post_connect: crate::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: crate::profile::SshMode::Embedded,
        x11_forwarding: false,
        remote_os: crate::profile::RemoteOs::Auto,
    }
}

fn rdp_profile() -> RdpProfile {
    RdpProfile {
        remote_os: crate::profile::RemoteOs::default(),
        extras: crate::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: false,
        redirect_drives: false,
        options: crate::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: crate::profile::Forwards::default(),
        follow_defaults: true,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

fn winrm_profile(username: Option<&str>) -> WinRmProfile {
    WinRmProfile {
        remote_os: crate::profile::RemoteOs::default(),
        id: ProfileId::new("ps"),
        name: "PS".to_owned(),
        group: None,
        host: "ps.lab".to_owned(),
        port: 5985,
        use_ssl: false,
        skip_certificate_check: false,
        username: username.map(str::to_owned),
        gateway: None,
        local_tunnel_port: None,
    }
}

#[test]
fn a_schedule_opens_only_what_opens_in_a_tab_without_asking() {
    let ssh = ssh_profile();
    assert_eq!(refusal(Target::Ssh(&ssh), true), None, "a plain SSH shell");
    for target in [Target::Telnet, Target::Vnc, Target::Ftp] {
        assert_eq!(refusal(target, false), None, "{target:?}");
    }
    assert_eq!(
        refusal(Target::Local, true),
        Some(RefusalKind::LocalProgram)
    );
    assert_eq!(refusal(Target::Citrix, true), Some(RefusalKind::Citrix));
    assert_eq!(refusal(Target::Web, true), Some(RefusalKind::WebConsole));
}

#[test]
fn an_ssh_profile_is_refused_in_putty_with_x11_or_with_steps_not_approved() {
    let mut putty = ssh_profile();
    putty.ssh_mode = crate::profile::SshMode::External;
    assert_eq!(
        refusal(Target::Ssh(&putty), true),
        Some(RefusalKind::ExternalClient)
    );
    let mut x11 = ssh_profile();
    x11.x11_forwarding = true;
    assert_eq!(
        refusal(Target::Ssh(&x11), true),
        Some(RefusalKind::X11Forwarding),
        "the local X server would start"
    );
    let mut steps = ssh_profile();
    steps.post_connect.steps = vec![crate::post_connect::PostConnectStep::new("uptime")];
    assert_eq!(
        refusal(Target::Ssh(&steps), true),
        Some(RefusalKind::StepsNotApproved)
    );
    steps.post_connect = crate::post_connect::PostConnect::approved_as(steps.post_connect.steps);
    assert_eq!(refusal(Target::Ssh(&steps), true), None, "approved: run");
    let mut sftp = ssh_profile();
    sftp.sftp = true;
    sftp.ssh_mode = crate::profile::SshMode::External;
    assert_eq!(
        refusal(Target::Ssh(&sftp), true),
        None,
        "its files, in a tab"
    );
    assert_eq!(
        refusal(Target::Ssh(&sftp), false),
        Some(RefusalKind::FilesBrowserOff)
    );
}

#[test]
fn an_rdp_profile_is_refused_outside_a_tab() {
    let rdp = rdp_profile();
    assert_eq!(refusal(Target::Rdp(&rdp), true), None);
    let mut external = rdp_profile();
    external.extras.external = true;
    assert_eq!(
        refusal(Target::Rdp(&external), true),
        Some(RefusalKind::ExternalClient)
    );
    let mut gateway = rdp_profile();
    gateway.extras.rd_gateway = Some("rdg.lab".to_owned());
    assert_eq!(
        refusal(Target::Rdp(&gateway), true),
        Some(RefusalKind::RdGateway)
    );
    gateway.extras.rd_gateway = Some("  ".to_owned());
    assert_eq!(refusal(Target::Rdp(&gateway), true), None, "none named");
}

#[test]
fn a_winrm_profile_needs_a_stored_account_and_password() {
    fn target(profile: &WinRmProfile, password_stored: bool) -> Target<'_> {
        Target::WinRm {
            profile,
            password_stored,
        }
    }
    let account = winrm_profile(Some(r"LAB\admin"));
    assert_eq!(refusal(target(&account, true), true), None);
    assert_eq!(
        refusal(target(&account, false), true),
        Some(RefusalKind::WinRmPasswordNotStored)
    );
    for identity in [winrm_profile(None), winrm_profile(Some(" "))] {
        assert_eq!(
            refusal(target(&identity, true), true),
            Some(RefusalKind::WinRmNoAccount),
            "the account running Heimdall"
        );
    }
}

#[test]
fn three_failures_in_a_row_turn_a_task_off_and_a_run_clears_them() {
    let mut task = task("t", None);
    task.failures = 0;
    assert!(!task.record(Outcome::Failed(FailureKind::ConnectionFailed)));
    assert!(!task.record(Outcome::Missed), "neither a run nor a failure");
    assert!(!task.record(Outcome::Deferred));
    assert!(!task.record(Outcome::Failed(FailureKind::ConnectionFailed)));
    assert_eq!(task.failures, 2);
    assert!(!task.record(Outcome::Ran));
    assert_eq!(task.failures, 0, "cleared");
    for _ in 1..MAX_CONSECUTIVE_FAILURES {
        assert!(!task.record(Outcome::Refused(RefusalKind::Citrix)));
    }
    assert!(task.enabled);
    assert!(
        task.record(Outcome::Failed(FailureKind::SessionLimit)),
        "turned off"
    );
    assert!(!task.enabled);
    assert!(task.suspended());
    assert_eq!(
        task.last_outcome,
        Some(Outcome::Failed(FailureKind::SessionLimit)),
        "the last reason kept"
    );
    assert!(
        !task.record(Outcome::Failed(FailureKind::SessionLimit)),
        "once"
    );
}
