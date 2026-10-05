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

//! The external credential provider's settings, as the C# Settings page's card.

use std::path::Path;

use heimdall_app::credential_provider::ProviderTest;
use heimdall_app::{App, AppConfig, Dialog, Effect, Message, ProviderMessage, SystemCredentials};
use heimdall_core::credential_provider::{PRESETS, ProviderKind};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;

fn app(dir: &Path, system: &SystemCredentials) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

fn send(app: &mut App, message: ProviderMessage) -> Vec<Effect> {
    app.update(Message::CredentialProvider(message))
}

fn saved(dir: &Path) -> Settings {
    Settings::load(&dir.join(SETTINGS_FILE_NAME)).expect("settings")
}

#[test]
fn each_change_is_saved_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    send(&mut app, ProviderMessage::Enabled(true));
    send(
        &mut app,
        ProviderMessage::Kind(ProviderKind::WindowsCredentialManager),
    );
    send(&mut app, ProviderMessage::Command("get {Title}".to_owned()));
    send(
        &mut app,
        ProviderMessage::UsernameCommand("user {Title}".to_owned()),
    );
    send(
        &mut app,
        ProviderMessage::Database("/v/team.kdbx".to_owned()),
    );
    send(
        &mut app,
        ProviderMessage::KeyFile("/v/team.keyx".to_owned()),
    );
    send(&mut app, ProviderMessage::FirstLineOnly(true));
    let provider = saved(dir.path()).credential_provider;
    assert!(provider.enabled && provider.first_line_only);
    assert_eq!(provider.kind, ProviderKind::WindowsCredentialManager);
    assert_eq!(
        (
            provider.command.as_str(),
            provider.username_command.as_str(),
            provider.database.as_str(),
            provider.key_file.as_str()
        ),
        (
            "get {Title}",
            "user {Title}",
            "/v/team.kdbx",
            "/v/team.keyx"
        )
    );
    assert_eq!(app.settings().credential_provider, provider);
}

#[test]
fn a_preset_writes_its_command() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    for (index, (_, template)) in PRESETS.iter().enumerate() {
        send(&mut app, ProviderMessage::Preset(index));
        assert_eq!(saved(dir.path()).credential_provider.command, *template);
    }
    send(&mut app, ProviderMessage::Preset(PRESETS.len()));
    assert_eq!(
        saved(dir.path()).credential_provider.command,
        PRESETS[PRESETS.len() - 1].1,
        "no preset there: nothing changes"
    );
}

#[test]
fn the_unlock_secret_is_kept_with_the_passwords_never_in_the_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    assert!(!app.provider_unlock_saved());
    send(
        &mut app,
        ProviderMessage::SaveUnlockSecret(Secret::new("db-pass".to_owned())),
    );
    assert!(app.provider_unlock_saved());
    let SystemCredentials::Memory(store) = &system else {
        panic!("memory");
    };
    let kept = store
        .lock()
        .expect("store")
        .get(heimdall_app::UNLOCK_SECRET_ENTRY)
        .map(|bytes| bytes.to_vec());
    assert_eq!(kept.as_deref(), Some(b"db-pass".as_slice()));
    let text = std::fs::read_to_string(dir.path().join(SETTINGS_FILE_NAME)).unwrap_or_default();
    assert!(!text.contains("db-pass"), "{text}");
    // Kept across restarts.
    drop(app);
    let mut app = self::app(dir.path(), &system);
    assert!(app.provider_unlock_saved());
    send(&mut app, ProviderMessage::ForgetUnlockSecret);
    assert!(!app.provider_unlock_saved());
    // An empty secret saved is none.
    send(
        &mut app,
        ProviderMessage::SaveUnlockSecret(Secret::new("x".to_owned())),
    );
    send(
        &mut app,
        ProviderMessage::SaveUnlockSecret(Secret::new(String::new())),
    );
    assert!(!app.provider_unlock_saved());
}

#[test]
fn the_unlock_secret_that_cannot_be_kept_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::Unavailable);
    send(
        &mut app,
        ProviderMessage::SaveUnlockSecret(Secret::new("db-pass".to_owned())),
    );
    assert!(
        matches!(app.dialog, Some(Dialog::PasswordSaveFailed { .. })),
        "{:?}",
        app.dialog
    );
    assert!(!app.provider_unlock_saved());
}

#[test]
fn test_runs_the_settings_as_they_are_with_the_unlock_secret() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    send(&mut app, ProviderMessage::Command("get {Title}".to_owned()));
    send(
        &mut app,
        ProviderMessage::SaveUnlockSecret(Secret::new("db-pass".to_owned())),
    );
    let effects = send(&mut app, ProviderMessage::Test);
    let [Effect::TestCredentialProvider { settings, unlock }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(settings.command, "get {Title}");
    assert_eq!(unlock.as_ref().map(Secret::expose), Some("db-pass"));
    assert_eq!(app.provider_test(), Some(&ProviderTest::Running));
    assert!(
        send(&mut app, ProviderMessage::Test).is_empty(),
        "one test at a time"
    );
    send(&mut app, ProviderMessage::Tested(ProviderTest::Success));
    assert_eq!(app.provider_test(), Some(&ProviderTest::Success));
}

#[test]
fn a_change_clears_the_test_and_a_late_result_is_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    send(&mut app, ProviderMessage::Test);
    send(&mut app, ProviderMessage::Command("other".to_owned()));
    assert_eq!(app.provider_test(), None);
    send(&mut app, ProviderMessage::Tested(ProviderTest::Success));
    assert_eq!(
        app.provider_test(),
        None,
        "the result of a command no longer set"
    );
    // The same value again changes nothing, the result stays.
    send(&mut app, ProviderMessage::Test);
    send(&mut app, ProviderMessage::Tested(ProviderTest::NoResult));
    send(&mut app, ProviderMessage::Command("other".to_owned()));
    assert_eq!(app.provider_test(), Some(&ProviderTest::NoResult));
}

#[test]
fn a_change_that_cannot_be_saved_is_not_kept() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir(dir.path().join(SETTINGS_FILE_NAME)).expect("folder");
    std::fs::create_dir(dir.path().join("settings.recovered.toml")).expect("folder");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    app.dialog = None;
    send(&mut app, ProviderMessage::Enabled(true));
    assert!(!app.settings().credential_provider.enabled);
    assert!(matches!(app.dialog, Some(Dialog::StoreError { .. })));
}

#[test]
fn the_command_timeout_is_chosen_within_the_csharp_range_and_saved() {
    use heimdall_core::credential_provider::{DEFAULT_TIMEOUT, MAX_TIMEOUT};
    use std::time::Duration;

    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    assert_eq!(app.settings().credential_provider.timeout, DEFAULT_TIMEOUT);
    send(&mut app, ProviderMessage::Timeout(Duration::from_secs(45)));
    assert_eq!(
        saved(dir.path()).credential_provider.timeout,
        Duration::from_secs(45)
    );
    for refused in [Duration::ZERO, MAX_TIMEOUT + Duration::from_secs(1)] {
        send(&mut app, ProviderMessage::Timeout(refused));
        assert_eq!(
            app.settings().credential_provider.timeout,
            Duration::from_secs(45),
            "{refused:?} refused"
        );
    }
}
