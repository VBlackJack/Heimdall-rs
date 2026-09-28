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

//! The application PIN, as the C# Heimdall's: asked at start before anything else, set,
//! changed and removed from the Settings page, wrong tries counted across runs.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, PinDialog, PinFailure, PinMessage, PinMode,
    SystemCredentials, VaultMode,
};
use heimdall_core::lockout::MAX_FAILED_ATTEMPTS;
use heimdall_core::pin::PinProblem;
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;

const PIN: &str = "2468";

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn secret(text: &str) -> Secret {
    Secret::new(text.to_owned())
}

fn saved(dir: &Path) -> Settings {
    Settings::load(&dir.join(SETTINGS_FILE_NAME)).expect("settings")
}

fn pin_dialog(app: &App) -> &PinDialog {
    match &app.dialog {
        Some(Dialog::Pin(dialog)) => dialog,
        other => panic!("{other:?}"),
    }
}

fn problem(app: &App) -> Option<PinFailure> {
    pin_dialog(app).problem.clone()
}

/// Sets `pin` from the Settings page, `current` typed as the PIN set.
fn configure(app: &mut App, current: &str, pin: &str, confirm: &str) {
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Save {
        current: secret(current),
        new: secret(pin),
        confirm: secret(confirm),
    }));
}

/// The application over `dir`, restarted with the PIN `PIN` set.
fn with_pin(dir: &Path) -> App {
    let mut app = app(dir);
    configure(&mut app, "", PIN, PIN);
    assert_eq!(app.dialog, None, "set");
    self::app(dir)
}

fn submit(app: &mut App, pin: &str) -> Vec<Effect> {
    app.update(Message::Pin(PinMessage::Submit(secret(pin))))
}

#[test]
fn without_a_pin_nothing_is_asked_at_start() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(app.dialog, None);
    assert!(!app.pin_asked());
}

#[test]
fn a_pin_set_is_asked_at_start_before_anything_else() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    assert!(app.pin_asked());
    assert_eq!(problem(&app), None);
    submit(&mut app, "1111");
    assert_eq!(
        problem(&app),
        Some(PinFailure::Wrong {
            remaining: MAX_FAILED_ATTEMPTS - 1
        })
    );
    assert!(app.pin_asked(), "still asked");
    submit(&mut app, PIN);
    assert_eq!(app.dialog, None);
    assert!(!app.pin_asked());
}

#[test]
fn cancelled_at_start_the_application_quits() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    let effects = app.update(Message::DismissDialog);
    assert!(matches!(effects.as_slice(), [Effect::Exit]), "{effects:?}");
}

#[test]
fn an_empty_pin_is_not_a_try() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    submit(&mut app, "");
    assert_eq!(problem(&app), None);
    assert_eq!(saved(dir.path()).pin_unlock.failures(), 0);
}

#[test]
fn the_fifth_wrong_pin_locks_out_across_restarts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    for tried in 1..MAX_FAILED_ATTEMPTS {
        submit(&mut app, "1111");
        assert_eq!(
            problem(&app),
            Some(PinFailure::Wrong {
                remaining: MAX_FAILED_ATTEMPTS - tried
            })
        );
    }
    submit(&mut app, "1111");
    assert!(matches!(problem(&app), Some(PinFailure::LockedOut { .. })));
    // Locked out, even the right PIN is not taken.
    submit(&mut app, PIN);
    assert!(app.pin_asked());
    assert!(matches!(problem(&app), Some(PinFailure::LockedOut { .. })));

    drop(app);
    assert!(saved(dir.path()).pin_unlock.until().is_some());
    let mut app = self::app(dir.path());
    assert!(
        matches!(problem(&app), Some(PinFailure::LockedOut { .. })),
        "said at once after a restart"
    );
    submit(&mut app, PIN);
    assert!(app.pin_asked(), "still locked out after a restart");
}

#[test]
fn a_right_pin_starts_the_count_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    for _ in 1..MAX_FAILED_ATTEMPTS {
        submit(&mut app, "1111");
    }
    assert_eq!(
        saved(dir.path()).pin_unlock.failures(),
        MAX_FAILED_ATTEMPTS - 1,
        "each wrong try saved at once"
    );
    submit(&mut app, PIN);
    assert_eq!(saved(dir.path()).pin_unlock.failures(), 0);
}

#[tokio::test]
async fn the_master_password_is_asked_once_the_pin_is_taken() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    submit(&mut app, PIN);
    // A vault made meanwhile: the next start asks the PIN, then the master password.
    app.update(Message::ShowVault);
    let effects = app.update(Message::SubmitVault {
        password: secret("correct horse battery staple"),
        new: None,
        confirm: Some(secret("correct horse battery staple")),
    });
    let [
        Effect::OpenVault {
            path,
            password,
            job,
        },
    ] = <[Effect; 1]>::try_from(effects).expect("one effect")
    else {
        panic!("expected OpenVault");
    };
    let opened = heimdall_app::open_vault(path, password, job).await;
    app.update(Message::VaultOpened(opened));
    drop(app);

    let mut app = self::app(dir.path());
    assert!(app.pin_asked(), "the PIN first");
    submit(&mut app, PIN);
    assert!(
        matches!(&app.dialog, Some(Dialog::Vault(vault)) if vault.mode == VaultMode::Unlock),
        "{:?}",
        app.dialog
    );
}

#[test]
fn a_dialog_the_start_had_to_show_waits_for_the_pin() {
    let dir = tempfile::tempdir().expect("dir");
    drop(with_pin(dir.path()));
    std::fs::write(dir.path().join("profiles.toml"), "not = [toml").expect("broken");
    let mut app = app(dir.path());
    assert!(app.pin_asked(), "the PIN before the error");
    submit(&mut app, PIN);
    assert!(
        matches!(&app.dialog, Some(Dialog::StoreError { .. })),
        "{:?}",
        app.dialog
    );
}

#[test]
fn a_new_pin_follows_the_rule_and_is_typed_twice_alike() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for (pin, confirm, refused) in [
        ("123", "123", PinFailure::Refused(PinProblem::TooShort)),
        (
            "123456789",
            "123456789",
            PinFailure::Refused(PinProblem::TooLong),
        ),
        ("12a4", "12a4", PinFailure::Refused(PinProblem::NotDigits)),
        ("1234", "1235", PinFailure::Mismatch),
    ] {
        configure(&mut app, "", pin, confirm);
        assert_eq!(problem(&app), Some(refused), "{pin} / {confirm}");
        app.update(Message::DismissDialog);
    }
    assert_eq!(saved(dir.path()).pin, None, "nothing set");
    // The rule is checked before the confirmation, as in C#.
    configure(&mut app, "", "12", "34");
    assert_eq!(
        problem(&app),
        Some(PinFailure::Refused(PinProblem::TooShort))
    );
}

#[test]
fn cancelled_from_the_settings_nothing_changes_and_the_application_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Pin(PinMessage::Configure));
    assert_eq!(
        pin_dialog(&app).mode,
        PinMode::Setup { current: false },
        "no current PIN asked when none is set"
    );
    let effects = app.update(Message::DismissDialog);
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(app.dialog, None);
}

#[test]
fn changing_the_pin_asks_the_current_one_and_counts_it_wrong() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    submit(&mut app, PIN);
    app.update(Message::Pin(PinMessage::Configure));
    assert_eq!(pin_dialog(&app).mode, PinMode::Setup { current: true });
    app.update(Message::Pin(PinMessage::Save {
        current: secret("1111"),
        new: secret("13579"),
        confirm: secret("13579"),
    }));
    assert_eq!(problem(&app), Some(PinFailure::WrongCurrent));
    assert_eq!(saved(dir.path()).pin_unlock.failures(), 1, "counted");
    app.update(Message::Pin(PinMessage::Save {
        current: secret(PIN),
        new: secret("13579"),
        confirm: secret("13579"),
    }));
    assert_eq!(app.dialog, None);
    let settings = saved(dir.path());
    let pin = settings.pin.expect("set");
    assert!(pin.verify("13579") && !pin.verify(PIN));
    assert_eq!(settings.pin_unlock.failures(), 0);
}

#[test]
fn the_settings_dialog_locks_out_as_the_start_does() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    submit(&mut app, PIN);
    app.update(Message::Pin(PinMessage::Configure));
    for _ in 0..MAX_FAILED_ATTEMPTS {
        app.update(Message::Pin(PinMessage::Remove(secret("1111"))));
    }
    assert!(matches!(problem(&app), Some(PinFailure::LockedOut { .. })));
    app.update(Message::Pin(PinMessage::Remove(secret(PIN))));
    assert!(
        saved(dir.path()).pin.is_some(),
        "not removed while locked out"
    );
}

#[test]
fn removing_the_pin_asks_it_and_the_next_start_asks_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_pin(dir.path());
    submit(&mut app, PIN);
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Remove(secret("1111"))));
    assert_eq!(problem(&app), Some(PinFailure::WrongCurrent));
    assert!(saved(dir.path()).pin.is_some());
    app.update(Message::Pin(PinMessage::Remove(secret(PIN))));
    assert_eq!(app.dialog, None);
    assert_eq!(saved(dir.path()).pin, None);
    let text = std::fs::read_to_string(dir.path().join(SETTINGS_FILE_NAME)).expect("text");
    assert!(!text.contains("salt"), "{text}");
    drop(app);
    assert!(!self::app(dir.path()).pin_asked());
}

#[test]
fn nothing_is_removed_or_changed_without_a_pin_set() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // A message not from the dialog does nothing.
    app.update(Message::Pin(PinMessage::Remove(secret(PIN))));
    app.update(Message::Pin(PinMessage::Save {
        current: secret(""),
        new: secret(PIN),
        confirm: secret(PIN),
    }));
    assert_eq!(app.dialog, None);
    assert_eq!(
        Settings::load(&dir.path().join(SETTINGS_FILE_NAME))
            .map(|s| s.pin)
            .ok()
            .flatten(),
        None
    );
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Remove(secret(PIN))));
    assert_eq!(pin_dialog(&app).problem, None, "no PIN to remove");
}

#[test]
fn a_pin_that_cannot_be_saved_is_not_set() {
    let dir = tempfile::tempdir().expect("dir");
    // Folders where the settings file and its recovery copy go: neither can be written.
    std::fs::create_dir(dir.path().join(SETTINGS_FILE_NAME)).expect("folder");
    std::fs::create_dir(dir.path().join("settings.recovered.toml")).expect("folder");
    let mut app = app(dir.path());
    app.dialog = None;
    configure(&mut app, "", PIN, PIN);
    assert!(
        matches!(problem(&app), Some(PinFailure::System { .. })),
        "{:?}",
        problem(&app)
    );
    app.update(Message::DismissDialog);
    app.update(Message::Pin(PinMessage::Configure));
    assert_eq!(
        pin_dialog(&app).mode,
        PinMode::Setup { current: false },
        "still no PIN"
    );
}
