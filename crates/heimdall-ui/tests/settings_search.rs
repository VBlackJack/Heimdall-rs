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

//! The Settings page's search, its "Modified" markers and their reset, and the security
//! overview, as the C# Settings tab has them.
//!
//! The texts are one loader for the whole process, and one test reads them in French: every
//! test here holds the same lock and starts in English, so none reads another's language.

mod common;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use heimdall_app::{App, AppConfig, Message as AppMessage, SettingsMessage, SystemCredentials};
use heimdall_core::profile::RdpDefaults;
use heimdall_core::settings::{
    Accent, AppTheme, ColorScheme, ExecutionPolicy, Language, Settings, SftpBrowser, settings_path,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::settings_rows::{SettingRow, SettingsCard};
use heimdall_ui::shell::{
    Destination, Message, SessionField, SettingsTab, Shell, settings_search_field_id,
};
use heimdall_ui::terminal_view::FONTS;
use iced::keyboard::key::Named;
use iced::{Settings as IcedSettings, Size};

/// A window tall enough for a whole tab of the Settings page.
const WINDOW: Size = Size::new(1100.0, 2400.0);

/// The language the texts are read in, held by one test at a time.
static LANGUAGE: Mutex<()> = Mutex::new(());

/// The texts held in English until dropped, and put back in English then, a test that
/// failed in French included.
struct English {
    _held: MutexGuard<'static, ()>,
}

impl Drop for English {
    fn drop(&mut self) {
        heimdall_ui::i18n::apply(Some(Language::English));
    }
}

fn english() -> English {
    let guard = LANGUAGE.lock().unwrap_or_else(PoisonError::into_inner);
    heimdall_ui::i18n::apply(Some(Language::English));
    English { _held: guard }
}

/// The Settings page of an application with every setting at its default.
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

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = IcedSettings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..IcedSettings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

fn change(shell: &mut Shell, message: SettingsMessage) {
    let _ = shell.update(Message::App(AppMessage::Settings(message)));
}

#[test]
fn the_search_finds_rows_by_name_hint_heading_or_choice_on_every_tab_whatever_the_case() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    // The General tab is shown: a name of the SSH tab is found all the same.
    assert_eq!(
        shell.settings_found("ssh keep-alive"),
        [SettingRow::KeepAlive]
    );
    assert!(
        shell
            .settings_found("dropped by a firewall")
            .contains(&SettingRow::KeepAlive),
        "what is said under it"
    );
    assert!(
        shell
            .settings_found("COLOR scheme")
            .contains(&SettingRow::ColorScheme),
        "whatever the case"
    );
    assert!(
        shell
            .settings_found("dracula")
            .contains(&SettingRow::ColorScheme),
        "a choice of its list, as the C# finds a list's items"
    );
    assert_eq!(
        shell.settings_found("SFTP browser"),
        SettingsCard::Sftp.rows(),
        "a card's heading finds its rows"
    );
    assert!(
        shell.settings_found("   ").is_empty(),
        "blank finds nothing"
    );

    let _ = shell.update(Message::SettingsSearch("keep-alive".to_owned()));
    let mut ui = simulator(&shell);
    for said in ["Results: 1", "SSH", "Session", "SSH keep-alive interval"] {
        ui.find(said).expect(said);
    }
    assert!(ui.find("Language").is_err(), "the tab is not shown");
    assert!(
        ui.find("TMOUT reset interval (0 = off)").is_err(),
        "the rows of the card not found are left out"
    );
}

#[test]
fn french_is_searched_in_french_without_its_accents() {
    let _english = english();
    heimdall_ui::i18n::apply(Some(Language::French));
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert!(
        shell
            .settings_found("delai d'expiration")
            .contains(&SettingRow::ReachabilityTimeout),
        "\"Délai\" found without its accent"
    );
    assert_eq!(
        shell.settings_found("EMPECHER LA MISE EN VEILLE"),
        [SettingRow::PreventSleep]
    );
    assert!(
        shell.settings_found("Probe timeout").is_empty(),
        "the English name is not searched"
    );
    let _ = shell.update(Message::SettingsSearch("aucun tel réglage".to_owned()));
    let mut ui = simulator(&shell);
    ui.find("Aucun réglage correspondant")
        .expect("the C# words for none");
}

#[test]
fn nothing_found_is_said_and_clearing_or_a_tab_shows_the_page_again() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSearch("no such setting".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("No matching settings")
            .expect("the C# words for none");
        assert!(ui.find("Language").is_err());
        ui.click("x").expect("the search's clear button");
        assert!(
            ui.into_messages().any(
                |message| matches!(message, Message::SettingsSearch(typed) if typed.is_empty())
            )
        );
    }
    let _ = shell.update(Message::SettingsSearch(String::new()));
    {
        let mut ui = simulator(&shell);
        ui.find("Language").expect("the General tab again");
        assert!(ui.find("No matching settings").is_err());
    }
    // A tab chosen while searching is shown whole.
    let _ = shell.update(Message::SettingsSearch("dracula".to_owned()));
    let _ = shell.update(Message::SettingsTab(SettingsTab::Terminal));
    let mut ui = simulator(&shell);
    for said in ["Terminal Appearance", "Session Logging", "Font size"] {
        ui.find(said).expect(said);
    }
}

#[test]
fn escape_empties_the_search_then_leaves_it() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSearch("dracula".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.click("dracula").expect("the search box");
        let _ = ui.tap_key(Named::Escape);
        assert!(
            ui.into_messages().any(
                |message| matches!(message, Message::SettingsSearch(typed) if typed.is_empty())
            )
        );
    }
    let _ = shell.update(Message::SettingsSearch(String::new()));
    let mut ui = simulator(&shell);
    ui.click("Search settings...")
        .expect("the search box, empty");
    let _ = ui.tap_key(Named::Escape);
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::SettingsSearch(_))),
        "empty, Escape only takes the keyboard from it"
    );
}

#[test]
fn ctrl_f_goes_to_the_settings_search_on_the_settings_page_only() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(shell.search_field(), settings_search_field_id());
    let _ = shell.update(Message::Navigate(Destination::Sessions));
    assert!(!shell.settings_shown());
    assert_ne!(
        shell.search_field(),
        settings_search_field_id(),
        "elsewhere the tree's search keeps it"
    );
}

#[test]
fn a_marker_shows_on_exactly_the_settings_off_their_default() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert!(
        SettingRow::ALL
            .iter()
            .all(|row| !row.is_modified(shell.app().settings()))
    );
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Modified").is_err(), "nothing changed yet");
    }
    change(&mut shell, SettingsMessage::ColorScheme(ColorScheme::Nord));
    change(&mut shell, SettingsMessage::SshAutoReconnect(true));
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "45".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::KeepAlive));
    change(
        &mut shell,
        SettingsMessage::SftpBrowser(SftpBrowser {
            follow_ssh_directory: true,
            ..SftpBrowser::default()
        }),
    );
    change(
        &mut shell,
        SettingsMessage::RdpDefaults(RdpDefaults {
            redirect_drives: true,
            ..RdpDefaults::default()
        }),
    );
    let expected = [
        SettingRow::ColorScheme,
        SettingRow::SshAutoReconnect,
        SettingRow::KeepAlive,
        SettingRow::SftpFollow,
        SettingRow::RdpDefaults,
    ];
    let marked: Vec<SettingRow> = SettingRow::ALL
        .into_iter()
        .filter(|row| row.is_modified(shell.app().settings()))
        .collect();
    assert_eq!(marked, expected);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Modified").is_err(), "none on the General tab");
    }
    let _ = shell.update(Message::SettingsTab(SettingsTab::Terminal));
    {
        let mut ui = simulator(&shell);
        ui.find("Modified").expect("the colour scheme's");
        ui.click("Reset").expect("its reset");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ResetSetting(SettingRow::ColorScheme)))
        );
    }
    // "Find modified settings" lists them, as the C# search for the marker's word.
    let _ = shell.update(Message::FindModifiedSettings);
    assert_eq!(shell.settings_found("Modified"), expected);
    let mut ui = simulator(&shell);
    ui.find("Results: 5").expect("their count");
}

#[test]
fn reset_puts_the_default_back_through_the_settings_and_saves_it() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let defaults = Settings::default();
    change(&mut shell, SettingsMessage::ColorScheme(ColorScheme::Nord));
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "45".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::KeepAlive));
    // Typed again, not applied: the reset drops it too.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "50".to_owned(),
    ));
    change(&mut shell, SettingsMessage::SessionLogging(true));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().settings().session_logging);

    for row in [
        SettingRow::ColorScheme,
        SettingRow::KeepAlive,
        SettingRow::SessionLogging,
        // No marker, no reset: nothing happens.
        SettingRow::Language,
    ] {
        let _ = shell.update(Message::ResetSetting(row));
    }
    assert!(shell.app().dialog.is_none(), "a reset asks nothing");
    let settings = shell.app().settings();
    assert_eq!(settings.color_scheme, defaults.color_scheme);
    assert_eq!(
        settings.ssh_keep_alive_interval,
        defaults.ssh_keep_alive_interval
    );
    assert!(!settings.session_logging);
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.color_scheme, defaults.color_scheme);
    assert_eq!(
        saved.ssh_keep_alive_interval,
        defaults.ssh_keep_alive_interval
    );
    assert!(!saved.session_logging);

    let _ = shell.update(Message::SettingsTab(SettingsTab::Ssh));
    let mut ui = simulator(&shell);
    assert!(ui.find("50").is_err(), "what was typed is gone");
    ui.find(defaults.ssh_keep_alive_interval.to_string().as_str())
        .expect("the default shown");
    assert!(ui.find("Modified").is_err());
}

#[test]
fn the_security_overview_reflects_the_settings_and_leads_to_them() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsTab(SettingsTab::Security));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Security overview",
            "No risky setting",
            "RDP Network Level Authentication (NLA): On",
            "RDP strict server authentication: Off",
            "Session transcripts: Off",
            "PowerShell execution policy: Default",
            "Master password: Disabled",
        ] {
            ui.find(said).expect(said);
        }
        assert!(ui.find("Go to setting").is_err(), "nothing needs attention");
    }
    change(
        &mut shell,
        SettingsMessage::RdpDefaults(RdpDefaults {
            nla: false,
            ..RdpDefaults::default()
        }),
    );
    change(&mut shell, SettingsMessage::SessionLogging(true));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    change(
        &mut shell,
        SettingsMessage::PowerShellExecutionPolicy(ExecutionPolicy::Unrestricted),
    );
    {
        let mut ui = simulator(&shell);
        for said in [
            "3 items need attention",
            "RDP Network Level Authentication (NLA): Off",
            "Without NLA, you sign in on a server that has not proved its identity.",
            "Session transcripts: On",
            "PowerShell execution policy: Unrestricted",
            "This policy turns off the script signing check in the PowerShell sessions \
             Heimdall opens.",
        ] {
            ui.find(said).expect(said);
        }
        ui.click("Go to setting").expect("the first line's");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::GoToSetting(SettingRow::RdpDefaults)))
        );
    }
    let _ = shell.update(Message::GoToSetting(SettingRow::PowerShellPolicy));
    {
        let mut ui = simulator(&shell);
        ui.find("PowerShell Execution Policy")
            .expect("the setting shown");
        assert!(ui.find("Security overview").is_err(), "on its own tab");
    }
    let _ = shell.update(Message::GoToSetting(SettingRow::RdpDefaults));
    let mut ui = simulator(&shell);
    ui.find("The options of every RDP server that uses the global defaults.")
        .expect("the RDP options");
}

#[test]
fn the_theme_and_accent_are_chosen_on_the_general_tab_applied_at_once_and_reset() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    {
        let mut ui = simulator(&shell);
        // A list's value is drawn by the list itself, not as a text to find.
        for said in ["Appearance", "Theme", "Accent"] {
            ui.find(said).expect(said);
        }
    }
    assert!(shell.theme().extended_palette().is_dark);
    assert_eq!(
        shell.settings_found("parchment"),
        [SettingRow::Theme],
        "a theme of its list"
    );
    assert_eq!(
        shell.settings_found("accent"),
        [SettingRow::Accent],
        "by its name"
    );
    assert!(
        shell.settings_found("dracula").contains(&SettingRow::Theme),
        "the theme and the terminal's scheme both offer Dracula"
    );

    change(&mut shell, SettingsMessage::Theme(AppTheme::Parchment));
    change(&mut shell, SettingsMessage::Accent(Accent::Blue));
    let theme = shell.theme();
    assert!(!theme.extended_palette().is_dark, "a light theme");
    assert_eq!(
        theme.palette().primary,
        heimdall_ui::themes::colors(AppTheme::Parchment).blue,
        "the accent chosen"
    );
    assert_eq!(
        shell.app().settings().color_scheme,
        ColorScheme::Dracula,
        "the terminals' scheme stays apart, as the C#"
    );
    let marked: Vec<SettingRow> = SettingRow::ALL
        .into_iter()
        .filter(|row| row.is_modified(shell.app().settings()))
        .collect();
    assert_eq!(marked, [SettingRow::Theme, SettingRow::Accent]);
    {
        let mut ui = simulator(&shell);
        ui.find("Modified").expect("their markers");
    }

    for row in [SettingRow::Theme, SettingRow::Accent] {
        let _ = shell.update(Message::ResetSetting(row));
    }
    let defaults = Settings::default();
    assert_eq!(shell.app().settings().theme, defaults.theme);
    assert_eq!(shell.app().settings().accent, defaults.accent);
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(
        (saved.theme, saved.accent),
        (AppTheme::Drakul, Accent::Default)
    );
    assert!(shell.theme().extended_palette().is_dark);
    let mut ui = simulator(&shell);
    assert!(ui.find("Modified").is_err());
}
