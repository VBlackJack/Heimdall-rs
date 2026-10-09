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

//! The look for a newer release, as the C# update banner: when it looks, what it offers,
//! and what the banner's buttons do. No request leaves the test: the look's answer is given.

use std::path::Path;
use std::time::{Duration, SystemTime};

use heimdall_app::update_check::{Failure, Outcome, ReleaseTag, START_DELAY, TICK};
use heimdall_app::{App, AppConfig, Effect, Message, SettingsMessage, UpdateMessage, UpdateStatus};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// The release the tests' build says it is.
const BUILT: &str = "v2026.100901";

fn tag(text: &str) -> ReleaseTag {
    ReleaseTag::parse(text).expect(text)
}

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

/// The application of a release build of [`BUILT`].
fn released(dir: &Path) -> App {
    App::new(config(dir)).with_release(Some(tag(BUILT)))
}

/// The settings as saved in `dir`.
fn saved(dir: &Path) -> Settings {
    Settings::load(&dir.join(SETTINGS_FILE_NAME)).expect("settings")
}

/// Settings in `dir` saying the last look was `ago`.
fn looked(dir: &Path, ago: Duration) {
    let at = SystemTime::now() - ago;
    let seconds = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    std::fs::write(
        dir.join(SETTINGS_FILE_NAME),
        format!("version = 1\n[update_check]\nlast_check = {seconds}\n"),
    )
    .expect("settings");
}

fn update(app: &mut App, message: UpdateMessage) -> Vec<Effect> {
    app.update(Message::Update(message))
}

/// Whether `effects` are one look.
fn looks(effects: &[Effect]) -> bool {
    matches!(effects, [Effect::CheckForUpdate])
}

#[test]
fn a_development_build_never_looks_nor_offers_and_says_why_to_check_now() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path())).with_release(None);
    assert_eq!(app.running_release(), None);
    assert_eq!(app.update_tick_interval(), None, "no tick at all");
    assert!(update(&mut app, UpdateMessage::Tick).is_empty());
    assert!(update(&mut app, UpdateMessage::CheckNow).is_empty());
    assert_eq!(app.update_status(), Some(UpdateStatus::NeedsRelease));
    // Even an answer, however it came, offers nothing.
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Latest(tag("v2099.010101"))),
    );
    assert_eq!(app.update_offer(false), None);
}

#[test]
fn the_first_look_waits_a_little_then_looks_hourly_once_the_interval_has_passed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    assert_eq!(app.update_tick_interval(), Some(START_DELAY));
    assert!(
        looks(&update(&mut app, UpdateMessage::Tick)),
        "never looked"
    );
    assert_eq!(app.update_tick_interval(), Some(TICK));
    assert!(
        update(&mut app, UpdateMessage::Tick).is_empty(),
        "one look at a time"
    );
    let _ = update(&mut app, UpdateMessage::Checked(Outcome::NoRelease));
    assert!(
        saved(dir.path()).update_check.last_check.is_some(),
        "no release yet is an answer, stamped"
    );
    assert!(
        update(&mut app, UpdateMessage::Tick).is_empty(),
        "within the interval"
    );
    // A periodic look tells the Settings page nothing.
    assert_eq!(app.update_status(), None);

    // A day ago, with the C# daily default: due.
    let dir = tempfile::tempdir().expect("dir");
    looked(dir.path(), Duration::from_hours(25));
    let mut app = released(dir.path());
    assert!(looks(&update(&mut app, UpdateMessage::Tick)));
    // Two hours ago: not due daily, due hourly.
    let dir = tempfile::tempdir().expect("dir");
    looked(dir.path(), Duration::from_hours(2));
    let mut app = released(dir.path());
    assert!(update(&mut app, UpdateMessage::Tick).is_empty());
    let _ = app.update(Message::Settings(SettingsMessage::UpdateInterval(1)));
    assert!(looks(&update(&mut app, UpdateMessage::Tick)));
}

#[test]
fn a_spent_quota_waits_for_the_interval_any_other_failure_looks_again_at_the_next_tick() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    let _ = update(&mut app, UpdateMessage::Tick);
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Failed(Failure::TimedOut)),
    );
    assert_eq!(saved(dir.path()).update_check.last_check, None);
    assert!(
        looks(&update(&mut app, UpdateMessage::Tick)),
        "looked again"
    );
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Failed(Failure::RateLimited { retry_after: None })),
    );
    assert!(saved(dir.path()).update_check.last_check.is_some());
    assert!(update(&mut app, UpdateMessage::Tick).is_empty());
}

#[test]
fn check_now_looks_whatever_the_interval_or_the_switch_and_says_what_it_found() {
    let dir = tempfile::tempdir().expect("dir");
    looked(dir.path(), Duration::from_mins(1));
    let mut app = released(dir.path());
    let _ = app.update(Message::Settings(SettingsMessage::UpdateChecks(false)));
    assert!(!saved(dir.path()).updates.enabled, "saved off");
    assert_eq!(app.update_tick_interval(), None, "no tick while off");
    assert!(update(&mut app, UpdateMessage::Tick).is_empty());
    assert!(looks(&update(&mut app, UpdateMessage::CheckNow)));
    assert_eq!(app.update_status(), Some(UpdateStatus::Checking));
    assert!(
        update(&mut app, UpdateMessage::CheckNow).is_empty(),
        "one look at a time"
    );
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Failed(Failure::AccessDenied)),
    );
    assert_eq!(
        app.update_status(),
        Some(UpdateStatus::Failed(Failure::AccessDenied))
    );
    for (latest, said) in [
        ("v2026.100901", UpdateStatus::UpToDate),
        ("v2026.093001", UpdateStatus::UpToDate),
        ("v2026.100902", UpdateStatus::Available(tag("v2026.100902"))),
    ] {
        let _ = update(&mut app, UpdateMessage::CheckNow);
        let _ = update(
            &mut app,
            UpdateMessage::Checked(Outcome::Latest(tag(latest))),
        );
        assert_eq!(app.update_status(), Some(said), "{latest}");
    }
}

#[test]
fn a_newer_release_is_offered_outside_full_screen_and_view_release_opens_its_pinned_page() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    let _ = update(&mut app, UpdateMessage::Tick);
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Latest(tag("v2026.101001"))),
    );
    assert_eq!(app.update_offer(false), Some(tag("v2026.101001")));
    assert_eq!(app.update_offer(true), None, "hidden in full screen");
    let opened = update(&mut app, UpdateMessage::ViewRelease);
    match opened.as_slice() {
        [Effect::OpenUrl(url)] => assert_eq!(
            url,
            "https://github.com/VBlackJack/Heimdall-rs/releases/tag/v2026.101001"
        ),
        other => panic!("expected the release page, got {other:?}"),
    }
    assert_eq!(
        app.update_offer(false),
        Some(tag("v2026.101001")),
        "still offered"
    );
}

#[test]
fn later_hides_the_banner_until_a_later_look_and_skip_for_that_release_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    let newer = || UpdateMessage::Checked(Outcome::Latest(tag("v2026.101001")));
    let _ = update(&mut app, newer());
    let _ = update(&mut app, UpdateMessage::Later);
    assert_eq!(app.update_offer(false), None);
    assert_eq!(saved(dir.path()).update_check.skipped, None);
    let _ = update(&mut app, newer());
    assert_eq!(
        app.update_offer(false),
        Some(tag("v2026.101001")),
        "found again"
    );

    let _ = update(&mut app, UpdateMessage::Skip);
    assert_eq!(app.update_offer(false), None);
    assert_eq!(
        saved(dir.path()).update_check.skipped.as_deref(),
        Some("v2026.101001")
    );
    let _ = update(&mut app, UpdateMessage::CheckNow);
    let _ = update(&mut app, newer());
    assert_eq!(app.update_offer(false), None, "skipped, not offered");
    assert_eq!(
        app.update_status(),
        Some(UpdateStatus::Available(tag("v2026.101001"))),
        "but Check now says it exists"
    );
    let _ = update(
        &mut app,
        UpdateMessage::Checked(Outcome::Latest(tag("v2026.101101"))),
    );
    assert_eq!(
        app.update_offer(false),
        Some(tag("v2026.101101")),
        "a newer one is"
    );
}

#[test]
fn an_interval_out_of_the_csharp_range_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    for refused in [0, 8761] {
        let _ = app.update(Message::Settings(SettingsMessage::UpdateInterval(refused)));
        assert_eq!(app.settings().updates.interval_hours, 24, "{refused}");
    }
    let _ = app.update(Message::Settings(SettingsMessage::UpdateInterval(8760)));
    assert_eq!(saved(dir.path()).updates.interval_hours, 8760);
}

#[test]
fn offer_it_again_forgets_the_release_skipped_and_the_next_look_offers_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = released(dir.path());
    let newer = || UpdateMessage::Checked(Outcome::Latest(tag("v2026.101001")));
    let _ = update(&mut app, newer());
    let _ = update(&mut app, UpdateMessage::Skip);
    assert_eq!(
        app.settings().update_check.skipped.as_deref(),
        Some("v2026.101001")
    );

    // As the C# `ClearSkippedVersion`: forgotten, and saved so.
    assert!(update(&mut app, UpdateMessage::ClearSkipped).is_empty());
    assert_eq!(app.settings().update_check.skipped, None);
    assert_eq!(saved(dir.path()).update_check.skipped, None);
    let _ = update(&mut app, newer());
    assert_eq!(
        app.update_offer(false),
        Some(tag("v2026.101001")),
        "offered again"
    );
    // Nothing skipped: nothing to forget, nothing written.
    let _ = update(&mut app, UpdateMessage::ClearSkipped);
    assert_eq!(app.settings().update_check.skipped, None);
}

#[test]
fn a_release_tag_says_its_build_date_as_the_csharp_derives_it_from_its_version() {
    assert_eq!(tag("v2026.100901").date().as_deref(), Some("2026-10-09"));
    assert_eq!(tag("v2024.022903").date().as_deref(), Some("2024-02-29"));
    assert_eq!(tag("v2026.123101").date().as_deref(), Some("2026-12-31"));
    // No such day: no date, as the C# `DateOnly` refuses it.
    for no_date in [
        "v2025.022901",
        "v2026.130101",
        "v2026.000101",
        "v2026.100001",
        "v2026.043101",
    ] {
        assert_eq!(tag(no_date).date(), None, "{no_date}");
    }
    assert_eq!(tag("v2000.022901").date().as_deref(), Some("2000-02-29"));
    assert_eq!(
        tag("v2100.022901").date(),
        None,
        "a century not a leap year"
    );
}
