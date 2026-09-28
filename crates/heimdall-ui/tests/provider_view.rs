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

//! The external credential provider's card on the Settings page, driven by its clicks and
//! typing.

use std::path::Path;
use std::time::Duration;

use heimdall_app::credential_provider::{ProviderFailure, ProviderTest};
use heimdall_app::{App, AppConfig, Message as AppMessage, ProviderMessage, SystemCredentials};
use heimdall_core::credential_provider::TemplateProblem;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;

/// A window tall enough for the whole Settings page.
const WINDOW: Size = Size::new(1100.0, 2800.0);

fn shell(dir: &Path) -> Shell {
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::ShowSettings);
    shell
}

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

fn apply(shell: &mut Shell, messages: Vec<Message>) {
    for message in messages {
        let _ = shell.update(message);
    }
}

fn click(shell: &mut Shell, label: &str) -> Vec<Message> {
    let messages: Vec<Message> = {
        let mut ui = simulator(shell);
        ui.click(label).expect(label);
        ui.into_messages().collect()
    };
    apply(shell, messages.clone());
    messages
}

fn enabled(dir: &Path) -> Shell {
    let mut shell = shell(dir);
    click(&mut shell, "Use external credential provider");
    assert!(shell.app().settings().credential_provider.enabled);
    shell
}

#[test]
fn off_the_card_says_how_to_turn_it_on() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    ui.find("External Credential Provider").expect("the card");
    ui.find("Enable 'Use external credential provider' to configure these options")
        .expect("the C# hint");
    assert!(ui.find("Provider command").is_err(), "nothing else yet");
}

#[test]
fn on_it_shows_the_csharp_fields() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = enabled(dir.path());
    let mut ui = simulator(&shell);
    for label in [
        // A pick list's text is not in the tree: its "Custom" is not searched for.
        "Quick setup preset",
        "Provider command",
        "Placeholders: {Host}  {Port}  {User}  {Title}  {Database}  {KeyFile}",
        "Username command (optional)",
        "Unlock secret",
        "Database path",
        "Key file path",
        "Test",
        "Use only the first line of output",
    ] {
        ui.find(label).expect(label);
    }
}

#[test]
fn typing_the_command_saves_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = enabled(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.click("e.g. keepassxc-cli show -s {title}")
            .expect("the command's field, by its placeholder");
        ui.typewrite("pass show x");
        ui.into_messages().collect()
    };
    apply(&mut shell, messages);
    assert_eq!(
        shell.app().settings().credential_provider.command,
        "pass show x"
    );
}

#[test]
fn the_unlock_secret_is_saved_then_only_said_to_be() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = enabled(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.click("Database master password or GPG passphrase")
            .expect("the secret's field");
        ui.typewrite("db-pass");
        ui.into_messages().collect()
    };
    assert!(
        messages
            .iter()
            .all(|message| !format!("{message:?}").contains("db-pass")),
        "never in a message's log"
    );
    apply(&mut shell, messages);
    click(&mut shell, "Save");
    assert!(shell.app().provider_unlock_saved());
    {
        let mut ui = simulator(&shell);
        ui.find("An unlock secret is saved.").expect("said");
        assert!(ui.find("db-pass").is_err(), "never shown");
    }
    click(&mut shell, "Forget");
    assert!(!shell.app().provider_unlock_saved());
}

#[test]
fn test_asks_the_core_and_shows_what_it_found() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = enabled(dir.path());
    let messages = click(&mut shell, "Test");
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::CredentialProvider(ProviderMessage::Test))
        )),
        "{messages:?}"
    );
    simulator(&shell).find("Testing...").expect("running");
    let again = click(&mut shell, "Test");
    assert!(
        again.is_empty(),
        "not pressed again while it runs: {again:?}"
    );
    for (outcome, said) in [
        (
            ProviderTest::Success,
            "Connection successful - password retrieved.".to_owned(),
        ),
        (
            ProviderTest::NoResult,
            "Command returned no output. Check the command and database path.".to_owned(),
        ),
        (
            ProviderTest::TimedOut,
            "Command timed out (10s). Check that the CLI tool is installed and responsive."
                .to_owned(),
        ),
        (
            ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::Empty)),
            "Enter a command template first.".to_owned(),
        ),
        (
            ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::NoKeyFile)),
            "Select a key file first.".to_owned(),
        ),
        (
            ProviderTest::Failed(ProviderFailure::Launch("not found".to_owned())),
            "Test failed: not found".to_owned(),
        ),
    ] {
        let _ = shell.update(Message::App(AppMessage::CredentialProvider(
            ProviderMessage::Test,
        )));
        let _ = shell.update(Message::App(AppMessage::CredentialProvider(
            ProviderMessage::Tested(outcome),
        )));
        simulator(&shell).find(said.as_str()).expect(&said);
    }
}

#[test]
fn the_timeout_said_is_the_one_set() {
    let dir = tempfile::tempdir().expect("dir");
    let settings_file = dir.path().join("settings.toml");
    std::fs::write(
        &settings_file,
        "version = 1\n[credential_provider]\nenabled = true\ntimeout_ms = 30000\n",
    )
    .expect("settings");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.app().settings().credential_provider.timeout,
        Duration::from_secs(30)
    );
    let _ = shell.update(Message::App(AppMessage::CredentialProvider(
        ProviderMessage::Test,
    )));
    let _ = shell.update(Message::App(AppMessage::CredentialProvider(
        ProviderMessage::Tested(ProviderTest::TimedOut),
    )));
    simulator(&shell)
        .find("Command timed out (30s). Check that the CLI tool is installed and responsive.")
        .expect("30 seconds");
}

#[test]
fn the_status_bar_says_why_the_provider_gave_nothing() {
    use heimdall_app::{Notice, SessionStatus};
    use heimdall_ui::status_bar::status_text;

    for (notice, said) in [
        (
            Notice::ProviderNoPassword("Web server".to_owned()),
            "The external credential provider returned no password for \"Web server\". \
             Check the command configuration in Settings > Security.",
        ),
        (
            Notice::ProviderFailed("not found".to_owned()),
            "External credential provider failed: not found",
        ),
        (
            Notice::ProviderTimedOut,
            "External credential provider timed out.",
        ),
    ] {
        let text = status_text(&SessionStatus::Ready, Some(&notice), 0);
        // Fluent marks the placed values with isolation characters.
        let text: String = text
            .chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect();
        assert_eq!(text, said);
    }
}
