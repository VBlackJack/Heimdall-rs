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

//! The migration of this computer's C# store carrying over its preferences, offered for
//! review and taken only once confirmed.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, ImportSummary, Message, SystemCredentials};
use heimdall_core::import::csharp_settings::PreferenceOutcome;
use heimdall_core::settings::AppTheme;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// The C# store's profiles.
const SERVERS: &str = r#"{"servers":[
    {"id":"r","displayName":"DC","remoteServer":"dc.lab","remotePort":3390,"connectionType":"RDP"},
    {"id":"x","displayName":"X","remoteServer":"x.lab","connectionType":"SSH","group":"Lab"}]}"#;

/// The C# `settings.json` of the tests: a preference changed, a secret, a value not read.
const SETTINGS: &str =
    r#"{"defaultTheme": "Vesper", "pinHash": "c2VjcmV0", "terminalFontSize": 300}"#;

/// The configuration of an application in `dir`, the C# store in `dir/legacy` holding
/// `SERVERS` and, when given, `settings`.
fn config(dir: &Path, settings: Option<&str>) -> AppConfig {
    let legacy = dir.join("legacy");
    std::fs::create_dir_all(&legacy).expect("legacy");
    std::fs::write(legacy.join("servers.json"), SERVERS).expect("servers");
    if let Some(settings) = settings {
        std::fs::write(legacy.join("settings.json"), settings).expect("settings");
    }
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: Some(legacy),
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }
}

/// The report of the import just run.
fn report(app: &App) -> &ImportSummary {
    match &app.dialog {
        Some(Dialog::ImportDone(summary)) => summary,
        other => panic!("expected ImportDone, got {other:?}"),
    }
}

#[test]
fn the_csharp_preferences_are_offered_for_review_and_taken_only_once_confirmed() {
    let dir = tempfile::tempdir().expect("dir");
    let config = config(dir.path(), Some(SETTINGS));
    let mut app = App::new(config.clone());
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert_eq!(summary.merged.added, 2, "the profiles first");
    assert_eq!(summary.settings_unreadable, None);
    let Some(Ok(preferences)) = &summary.preferences else {
        panic!("{:?}", summary.preferences);
    };
    assert_eq!(preferences.taken, 1);
    assert!(preferences.review.is_some());
    assert_eq!(
        preferences
            .left_out(PreferenceOutcome::Secret)
            .collect::<Vec<_>>(),
        ["PinHash"]
    );
    assert_eq!(
        preferences
            .left_out(PreferenceOutcome::Invalid)
            .collect::<Vec<_>>(),
        ["TerminalFontSize"]
    );
    assert_eq!(
        app.settings().theme,
        AppTheme::default(),
        "nothing taken before the review"
    );

    app.update(Message::ReviewCsharpPreferences);
    let Some(Dialog::ConfirmSettingsImport(review)) = &app.dialog else {
        panic!("expected the review, got {:?}", app.dialog);
    };
    let changed: Vec<&str> = review
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert_eq!(changed, ["general.theme"]);
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().theme, AppTheme::Vesper);
    assert_eq!(app.settings().pin, None, "no secret crossed");
    assert_eq!(App::new(config).settings().theme, AppTheme::Vesper, "saved");
}

#[test]
fn a_review_cancelled_takes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), Some(SETTINGS)));
    app.update(Message::ImportLegacy);
    app.update(Message::ReviewCsharpPreferences);
    app.update(Message::DismissDialog);
    assert_eq!(app.settings().theme, AppTheme::default());
    // Nothing to review once the report is gone.
    app.update(Message::ReviewCsharpPreferences);
    assert_eq!(app.dialog, None);
}

#[test]
fn a_settings_file_that_is_no_json_never_stops_the_profiles_and_is_said_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), Some(r#"{"defaultTheme": "Vesper", "#)));
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert_eq!(summary.merged.added, 2, "imported without the settings");
    assert!(summary.settings_unreadable.is_some(), "{summary:?}");
    assert_eq!(
        summary.preferences, None,
        "the same failure, not said twice"
    );
    assert_eq!(app.settings().theme, AppTheme::default());
}

#[test]
fn a_part_the_profiles_cannot_read_leaves_the_preferences_read() {
    let dir = tempfile::tempdir().expect("dir");
    // Group defaults of another shape: the profiles are imported without them.
    let settings = r#"{"defaultTheme": "Vesper", "groupDefaults": ["not", "a", "map"]}"#;
    let mut app = App::new(config(dir.path(), Some(settings)));
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert_eq!(summary.merged.added, 2);
    assert!(summary.settings_unreadable.is_some(), "{summary:?}");
    assert!(
        matches!(&summary.preferences, Some(Ok(preferences)) if preferences.taken == 1),
        "{:?}",
        summary.preferences
    );
}

#[test]
fn without_a_settings_file_nothing_is_said_of_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), None));
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert_eq!(summary.merged.added, 2);
    assert_eq!(
        (&summary.settings_unreadable, &summary.preferences),
        (&None, &None)
    );
}
