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

//! The application PIN drawn: the gate at start hiding the window, the Settings card, and
//! the dialog setting, changing and removing it, each driven by its clicks and keys.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, Message as AppMessage, PinMessage, SystemCredentials};
use heimdall_core::lockout::MAX_FAILED_ATTEMPTS;
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::keyboard::key::Named;
use iced::{Settings, Size};

/// A window tall enough for the whole Settings page.
const WINDOW: Size = Size::new(1100.0, 1800.0);
const PIN: &str = "2468";

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "production web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
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
    }]);
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

/// The window over `dir`, started with the PIN `PIN` set.
fn gated(dir: &Path) -> Shell {
    let mut core = app(dir);
    core.update(AppMessage::Pin(PinMessage::Configure));
    core.update(AppMessage::Pin(PinMessage::Save {
        current: Secret::new(String::new()),
        new: Secret::new(PIN.to_owned()),
        confirm: Secret::new(PIN.to_owned()),
    }));
    assert_eq!(core.dialog, None, "set");
    Shell::with_app(app(dir))
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

fn field(index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("vault-field-{index}"))
}

/// Types `text` into field `index` of the dialog, as the user does, and applies it.
fn type_into(shell: &mut Shell, index: usize, text: &str) {
    let messages: Vec<Message> = {
        let mut ui = simulator(shell);
        ui.click(field(index)).expect("the field");
        ui.typewrite(text);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
}

/// Clicks `label` and applies what it produces; the messages, for the test to read.
fn click(shell: &mut Shell, label: &str) -> Vec<String> {
    let messages: Vec<Message> = {
        let mut ui = simulator(shell);
        ui.click(label).expect(label);
        ui.into_messages().collect()
    };
    let names = messages
        .iter()
        .map(|message| format!("{message:?}"))
        .collect();
    for message in messages {
        let _ = shell.update(message);
    }
    names
}

#[test]
fn at_start_the_pin_hides_the_window_until_it_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = gated(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.find("Enter PIN").expect("asked");
        assert!(
            ui.find("production web").is_err(),
            "the profiles are not shown"
        );
        assert!(ui.find("Settings").is_err(), "nor the window's buttons");
    }
    type_into(&mut shell, 0, "1111");
    click(&mut shell, "Unlock");
    simulator(&shell)
        .find("Incorrect PIN. 4 attempts remaining.")
        .expect("the C# words");
    type_into(&mut shell, 0, PIN);
    click(&mut shell, "Unlock");
    assert!(!shell.app().pin_asked());
    simulator(&shell)
        .find("production web")
        .expect("the window, once the PIN is taken");
}

#[test]
fn enter_in_the_pin_field_tries_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = gated(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.click(field(0)).expect("the field");
        ui.typewrite(PIN);
        ui.tap_key(iced::keyboard::Key::Named(Named::Enter));
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
    assert!(!shell.app().pin_asked());
}

#[test]
fn window_shortcuts_do_nothing_behind_the_pin() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = gated(dir.path());
    let _ = shell.update(Message::Shortcut(
        heimdall_ui::terminal_view::keys::WindowShortcut::Broadcast,
    ));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Security,
    ));
    simulator(&shell)
        .find("Enter PIN")
        .expect("still the PIN, nothing else");
    assert!(shell.app().pin_asked());
}

#[test]
fn locked_out_the_unlock_button_takes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = gated(dir.path());
    for _ in 0..MAX_FAILED_ATTEMPTS {
        type_into(&mut shell, 0, "1111");
        click(&mut shell, "Unlock");
    }
    simulator(&shell)
        .find("Too many incorrect attempts. Try again in 5 minutes.")
        .expect("the C# words");
    type_into(&mut shell, 0, PIN);
    let sent = click(&mut shell, "Unlock");
    assert!(
        !sent.iter().any(|message| message == "SubmitPin"),
        "{sent:?}"
    );
    assert!(shell.app().pin_asked());
}

#[test]
fn the_settings_card_sets_a_pin_in_two_fields() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Security,
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("Application PIN").expect("the card");
        ui.find("No PIN is set.").expect("its state");
    }
    click(&mut shell, "Configure PIN...");
    {
        let mut ui = simulator(&shell);
        ui.find("Configure PIN").expect("the dialog");
        ui.find("New PIN").expect("new");
        ui.find("Confirm PIN").expect("confirm");
        assert!(ui.find("Current PIN").is_err(), "none to ask");
        assert!(ui.find("Remove PIN").is_err(), "none to remove");
    }
    type_into(&mut shell, 0, "13579");
    type_into(&mut shell, 1, "13579");
    click(&mut shell, "Save");
    assert_eq!(shell.app().dialog, None);
    assert!(
        shell
            .app()
            .settings()
            .pin
            .as_ref()
            .is_some_and(|pin| pin.verify("13579"))
    );
    simulator(&shell)
        .find("A PIN is currently set.")
        .expect("the card's new state");
}

#[test]
fn a_pin_set_is_asked_before_it_is_changed_or_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = gated(dir.path());
    type_into(&mut shell, 0, PIN);
    click(&mut shell, "Unlock");
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Security,
    ));
    click(&mut shell, "Configure PIN...");
    simulator(&shell)
        .find("Current PIN")
        .expect("the current one asked");
    type_into(&mut shell, 0, "1111");
    click(&mut shell, "Remove PIN");
    simulator(&shell)
        .find("Current PIN is incorrect.")
        .expect("the C# words");
    type_into(&mut shell, 0, PIN);
    click(&mut shell, "Remove PIN");
    assert_eq!(shell.app().dialog, None);
    assert_eq!(shell.app().settings().pin, None);
}

#[test]
fn a_new_pin_refused_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Security,
    ));
    for (pin, confirm, said) in [
        ("12", "12", "PIN must be at least 4 digits."),
        ("123456789", "123456789", "PIN must be at most 8 digits."),
        ("12ab", "12ab", "PIN must contain digits only."),
        ("1234", "4321", "The PINs do not match."),
        // The rule is the new PIN's: a short confirmation is only a mismatch.
        ("1234", "12", "The PINs do not match."),
    ] {
        click(&mut shell, "Configure PIN...");
        type_into(&mut shell, 0, pin);
        type_into(&mut shell, 1, confirm);
        click(&mut shell, "Save");
        simulator(&shell).find(said).expect(said);
        click(&mut shell, "Cancel");
    }
    assert_eq!(shell.app().settings().pin, None);
}
