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
    Accent, AppTheme, ColorScheme, ExecutionPolicy, Language, RDP_KEEP_ALIVE_INTERVAL_DEFAULT_MS,
    RDP_KEEP_ALIVE_INTERVAL_MAX_MS, RDP_KEEP_ALIVE_INTERVAL_MIN_MS, Settings, SftpBrowser,
    settings_path,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::settings_rows::{SettingRow, SettingsCard, SettingsSection};
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

/// An application with every setting at its default.
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

/// The Settings page of `app`.
fn settings_of(app: App) -> Shell {
    let mut shell = Shell::with_app(app);
    let _ = shell.update(Message::ShowSettings);
    shell
}

/// The Settings page of an application with every setting at its default.
fn shell(dir: &Path) -> Shell {
    settings_of(app(dir))
}

/// Creates the vault of `core` as its dialog would, the key derivation run to its end.
fn create_vault(core: &mut App) {
    use heimdall_app::{Effect, open_vault};
    use heimdall_ssh::Secret;

    const MASTER: &str = "correct horse battery staple";
    core.update(AppMessage::ShowVault);
    let effects = core.update(AppMessage::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: Some(Secret::new(MASTER.to_owned())),
    });
    let Ok(
        [
            Effect::OpenVault {
                path,
                password,
                job,
                ticket,
            },
        ],
    ) = <[Effect; 1]>::try_from(effects)
    else {
        panic!("expected OpenVault");
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime.block_on(open_vault(path, password, job));
    core.update(AppMessage::VaultOpened(ticket, result));
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
    assert_eq!(
        shell.settings_found("stabilization delay"),
        [SettingRow::RdpResizeDelay],
        "the C# RDP session card's wait after connecting"
    );
    assert_eq!(
        SettingRow::RdpResizeDelay.card(),
        SettingsCard::RdpSession,
        "beside the watchdog"
    );
    assert_eq!(
        shell.settings_found("default resolution width"),
        [SettingRow::RdpDefaultWidth],
        "the C# default resolution, used by Remote Desktop Connection"
    );
    assert_eq!(
        SettingRow::RdpDefaultHeight.card(),
        SettingsCard::RdpSession,
    );
    assert_eq!(
        shell.settings_found("session keep-alive"),
        [SettingRow::RdpKeepAlive],
        "the C# RDP Behavior card's keep-alive"
    );
    assert_eq!(
        SettingRow::RdpKeepAlive.card(),
        SettingsCard::RdpSession,
        "last of the card, as in the C#"
    );

    let _ = shell.update(Message::SettingsSearch("ssh keep-alive".to_owned()));
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
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Terminal),
    ));
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
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Terminal),
    ));
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
fn a_number_is_applied_once_typed_in_its_range_without_enter() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let defaults = Settings::default();
    let saved = || Settings::load(&settings_path(&dir.path().join("profiles.toml")));
    let _ = shell.update(Message::SettingsSection(SettingsSection::SshSession));
    // Typed key by key, as the C# field bound on `PropertyChanged` takes it: "4" is out of
    // the range, so stays typed with its rule and changes nothing.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "4".to_owned(),
    ));
    assert_eq!(
        shell.app().settings().ssh_keep_alive_interval,
        defaults.ssh_keep_alive_interval
    );
    let rule = "SSH keep-alive interval must be between 5 and 600 seconds.";
    {
        let mut ui = simulator(&shell);
        ui.find(rule).expect("its rule while out of it");
    }
    // "45" is in it: applied and saved at once, so leaving the field with Tab keeps it.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "45".to_owned(),
    ));
    assert_eq!(shell.app().settings().ssh_keep_alive_interval, 45);
    assert_eq!(saved().expect("saved").ssh_keep_alive_interval, 45);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find(rule).is_err(), "no rule once in the range");
        ui.find("45").expect("the value set shown");
    }
    // Set elsewhere since, Enter on the field never brings back what was typed.
    change(&mut shell, SettingsMessage::SshKeepAliveInterval(60));
    let _ = shell.update(Message::SessionFieldApply(SessionField::KeepAlive));
    assert_eq!(shell.app().settings().ssh_keep_alive_interval, 60);

    // The terminals' font size the same way.
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Terminal),
    ));
    let _ = shell.update(Message::FontSizeEdited("7".to_owned()));
    assert_eq!(
        shell.app().settings().terminal_font_size,
        defaults.terminal_font_size
    );
    let _ = shell.update(Message::FontSizeEdited("19".to_owned()));
    assert_eq!(shell.app().settings().terminal_font_size, 19);
    assert_eq!(saved().expect("saved").terminal_font_size, 19);
}

#[test]
fn the_rdp_keep_alive_is_applied_once_typed_in_its_range_and_never_on_the_way() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let file = settings_path(&dir.path().join("profiles.toml"));
    let _ = shell.update(Message::SettingsSection(
        SettingRow::RdpKeepAlive
            .card()
            .section()
            .expect("on the RDP rail"),
    ));
    let rule = format!(
        "RDP keep-alive interval must be between {RDP_KEEP_ALIVE_INTERVAL_MIN_MS} and {RDP_KEEP_ALIVE_INTERVAL_MAX_MS} ms."
    );
    let before = std::fs::read(&file).ok();
    // Typed toward 5000 key by key: "5", "50" and "500" are under the range, so none is
    // applied nor saved, its rule said under it.
    for typed in ["5", "50", "500"] {
        let _ = shell.update(Message::SessionFieldEdited(
            SessionField::RdpKeepAlive,
            typed.to_owned(),
        ));
        assert_eq!(
            shell.app().settings().rdp_keep_alive_interval_ms,
            RDP_KEEP_ALIVE_INTERVAL_DEFAULT_MS,
            "{typed}"
        );
        assert_eq!(std::fs::read(&file).ok(), before, "{typed} saved nothing");
        let mut ui = simulator(&shell);
        ui.find(rule.as_str()).expect(typed);
    }
    // "5000" is in it: applied and saved without Enter.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::RdpKeepAlive,
        "5000".to_owned(),
    ));
    assert_eq!(shell.app().settings().rdp_keep_alive_interval_ms, 5000);
    assert_eq!(
        Settings::load(&file)
            .expect("saved")
            .rdp_keep_alive_interval_ms,
        5000
    );
    {
        let mut ui = simulator(&shell);
        assert!(ui.find(rule.as_str()).is_err(), "no rule once in the range");
    }
    // Unchanged, neither Enter nor the same value typed again saves it again.
    std::fs::remove_file(&file).expect("removed");
    let _ = shell.update(Message::SessionFieldApply(SessionField::RdpKeepAlive));
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::RdpKeepAlive,
        "5000".to_owned(),
    ));
    assert!(!file.exists(), "the same value is not saved again");
    // Enter applies one typed in the range too.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::RdpKeepAlive,
        "300001".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::RdpKeepAlive));
    assert_eq!(shell.app().settings().rdp_keep_alive_interval_ms, 5000);
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::RdpKeepAlive,
        "300000".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::RdpKeepAlive));
    assert_eq!(
        shell.app().settings().rdp_keep_alive_interval_ms,
        RDP_KEEP_ALIVE_INTERVAL_MAX_MS
    );
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
    // Typed again out of its range, so not applied: the reset drops it too.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::KeepAlive,
        "601".to_owned(),
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

    let _ = shell.update(Message::SettingsSection(SettingsSection::SshSession));
    let mut ui = simulator(&shell);
    assert!(ui.find("601").is_err(), "what was typed is gone");
    ui.find(defaults.ssh_keep_alive_interval.to_string().as_str())
        .expect("the default shown");
    assert!(ui.find("Modified").is_err());
}

#[test]
fn the_security_overview_reflects_the_settings_and_leads_to_them() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
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
        (AppTheme::Magellan, Accent::Default)
    );
    assert!(shell.theme().extended_palette().is_dark);
    let mut ui = simulator(&shell);
    assert!(ui.find("Modified").is_err());
}

#[test]
fn without_a_master_password_the_lock_settings_say_so_and_cannot_change() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert!(
        shell
            .settings_found("auto-lock")
            .contains(&SettingRow::AutoLock)
    );
    assert_eq!(
        shell.settings_found("disconnect sessions"),
        [SettingRow::DisconnectOnLock]
    );
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    {
        let mut ui = simulator(&shell);
        for said in [
            "No risky setting",
            "Auto-lock when idle: Needs the master password",
            "Disconnect sessions when locking: Needs the master password",
        ] {
            ui.find(said).expect(said);
        }
    }
    let _ = shell.update(Message::SettingsSection(SettingsSection::Vault));
    let mut ui = simulator(&shell);
    for said in [
        "Auto-lock and disconnect on lock need the master password above: turn it on to use \
         them.",
        "Auto-lock after idle (0 = off)",
    ] {
        ui.find(said).expect(said);
    }
    ui.click("Disconnect sessions when locking")
        .expect("its box, greyed");
    assert!(
        !ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::DisconnectOnLock(_)))
        )),
        "nothing to change without a master password"
    );
}

#[test]
fn the_lock_settings_are_marked_reset_and_shown_in_the_security_overview() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    let mut shell = settings_of(core);
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    {
        let mut ui = simulator(&shell);
        for said in [
            "1 item needs attention",
            "Auto-lock when idle: Never",
            "The master password stays unlocked for as long as Heimdall runs.",
            "Disconnect sessions when locking: Off",
        ] {
            ui.find(said).expect(said);
        }
        assert!(
            ui.find("Auto-lock and disconnect on lock need the master password above: turn it on to use them.")
                .is_err(),
            "the master password is set"
        );
        assert!(ui.find("Modified").is_err(), "nothing changed yet");
        ui.click("Go to setting").expect("the idle lock's");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::GoToSetting(SettingRow::AutoLock)))
        );
    }
    let _ = shell.update(Message::GoToSetting(SettingRow::AutoLock));

    // Out of the C# range, it stays typed, its rule said, and nothing changes.
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::AutoLock,
        "1441".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::AutoLock));
    assert_eq!(shell.app().settings().auto_lock_idle_minutes, 0);
    {
        let mut ui = simulator(&shell);
        ui.find("Idle auto-lock threshold must be between 0 and 1440 minutes.")
            .expect("the C# rule");
    }
    let _ = shell.update(Message::SessionFieldEdited(
        SessionField::AutoLock,
        "15".to_owned(),
    ));
    let _ = shell.update(Message::SessionFieldApply(SessionField::AutoLock));
    change(&mut shell, SettingsMessage::DisconnectOnLock(true));
    let settings = shell.app().settings();
    assert_eq!(settings.auto_lock_idle_minutes, 15);
    assert!(settings.disconnect_on_lock);
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.auto_lock_idle_minutes, 15);
    assert!(saved.disconnect_on_lock);
    assert_eq!(
        shell.settings_found("Modified"),
        [SettingRow::AutoLock, SettingRow::DisconnectOnLock]
    );
    let _ = shell.update(Message::SettingsSection(SettingsSection::Overview));
    {
        let mut ui = simulator(&shell);
        for said in [
            "No risky setting",
            "Auto-lock when idle: After 15 minutes of inactivity",
            "Disconnect sessions when locking: On",
        ] {
            ui.find(said).expect(said);
        }
    }

    for row in [SettingRow::AutoLock, SettingRow::DisconnectOnLock] {
        let _ = shell.update(Message::ResetSetting(row));
    }
    let settings = shell.app().settings();
    assert_eq!(settings.auto_lock_idle_minutes, 0);
    assert!(!settings.disconnect_on_lock);
}

#[test]
fn ctrl_k_in_a_terminal_is_chosen_found_marked_and_reset_on_the_terminal_tab() {
    use heimdall_core::settings::CtrlKTerminal;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(shell.settings_found("ctrl+k"), [SettingRow::CtrlKTerminal]);
    assert!(
        shell
            .settings_found("quick connect")
            .contains(&SettingRow::CtrlKTerminal),
        "a choice of its list"
    );
    assert_eq!(SettingRow::CtrlKTerminal.card(), SettingsCard::Terminal);
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Terminal),
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("Ctrl+K in a terminal").expect("its row");
    }
    change(
        &mut shell,
        SettingsMessage::CtrlKTerminal(CtrlKTerminal::SendToSession),
    );
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.ctrl_k_terminal, CtrlKTerminal::SendToSession);
    assert_eq!(
        shell.settings_found("modified"),
        [SettingRow::CtrlKTerminal]
    );
    {
        let mut ui = simulator(&shell);
        ui.find("Modified").expect("its marker");
    }

    let _ = shell.update(Message::ResetSetting(SettingRow::CtrlKTerminal));
    assert_eq!(
        shell.app().settings().ctrl_k_terminal,
        CtrlKTerminal::QuickConnect
    );
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.ctrl_k_terminal, CtrlKTerminal::QuickConnect);
    assert!(!SettingRow::CtrlKTerminal.is_modified(shell.app().settings()));
}

#[test]
fn putty_and_the_x_server_are_typed_applied_found_and_reset_on_the_ssh_tab() {
    use heimdall_ui::settings_rows::ToolPath;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(shell.settings_found("putty path"), [SettingRow::PuttyPath]);
    assert_eq!(
        shell.settings_found("X11 server"),
        SettingsCard::X11.rows(),
        "the card's heading finds its rows"
    );
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Ssh),
    ));
    simulator(&shell)
        .find("PuTTY path (for External SSH mode)")
        .expect("on Connection, the SSH rail's first");
    let _ = shell.update(Message::SettingsSection(SettingsSection::SftpX11));
    {
        let mut ui = simulator(&shell);
        for said in ["X11 server path", "Auto-start X11 server when needed"] {
            ui.find(said).expect(said);
        }
    }
    // Typed, nothing applied until Enter.
    let _ = shell.update(Message::ToolPathEdited(
        ToolPath::Putty,
        " /opt/putty/putty ".to_owned(),
    ));
    assert!(shell.app().settings().putty_path.is_empty());
    let _ = shell.update(Message::ToolPathApply(ToolPath::Putty));
    assert_eq!(shell.app().settings().putty_path, "/opt/putty/putty");
    let _ = shell.update(Message::ToolPathEdited(
        ToolPath::X11Server,
        "/opt/x/vcxsrv".to_owned(),
    ));
    let _ = shell.update(Message::ToolPathApply(ToolPath::X11Server));
    change(&mut shell, SettingsMessage::X11AutoStart(false));
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.putty_path, "/opt/putty/putty");
    assert_eq!(saved.x11_server_path, "/opt/x/vcxsrv");
    assert!(!saved.x11_auto_start);
    let modified: Vec<SettingRow> = shell
        .settings_found("modified")
        .into_iter()
        .filter(|row| row.tab() == SettingsTab::Ssh)
        .collect();
    assert_eq!(
        modified,
        [
            SettingRow::PuttyPath,
            SettingRow::X11ServerPath,
            SettingRow::X11AutoStart
        ]
    );

    for row in [
        SettingRow::PuttyPath,
        SettingRow::X11ServerPath,
        SettingRow::X11AutoStart,
    ] {
        let _ = shell.update(Message::ResetSetting(row));
    }
    let defaults = Settings::default();
    let settings = shell.app().settings();
    assert_eq!(settings.putty_path, defaults.putty_path);
    assert_eq!(settings.x11_server_path, defaults.x11_server_path);
    assert!(settings.x11_auto_start);
}

#[test]
fn the_default_ssh_mode_is_chosen_found_reset_and_applied_to_all_once_asked() {
    use heimdall_core::profile::{ProfileId, SshMode, SshProfile};
    use heimdall_core::store::ProfileStore;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
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
    }]);
    store.save().expect("save");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("default ssh mode"),
        [SettingRow::SshDefaultMode]
    );
    assert!(
        shell
            .settings_found("apply to all")
            .contains(&SettingRow::SshDefaultMode),
        "its button"
    );
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Ssh),
    ));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Default SSH mode",
            "Embedded: terminal runs inside Heimdall. External: opens PuTTY in a separate window.",
            "Apply to all saved sessions",
        ] {
            ui.find(said).expect(said);
        }
        assert!(ui.find("Modified").is_err(), "the default");
    }

    change(
        &mut shell,
        SettingsMessage::SshDefaultMode(SshMode::External),
    );
    assert_eq!(
        shell.settings_found("modified"),
        [SettingRow::SshDefaultMode]
    );
    {
        let mut ui = simulator(&shell);
        ui.click("Apply to all saved sessions").expect("the button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::ApplySshModeToAll))
        )));
    }
    change(&mut shell, SettingsMessage::ApplySshModeToAll);
    {
        let mut ui = simulator(&shell);
        for said in [
            "Apply to all saved SSH sessions?",
            "1 of 1 saved SSH sessions will switch to the External mode, and External becomes \
             the default for new sessions. This cannot be undone.",
        ] {
            ui.find(said).expect(said);
        }
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert_eq!(shell.app().profiles()[0].ssh_mode, SshMode::External);

    let _ = shell.update(Message::ResetSetting(SettingRow::SshDefaultMode));
    assert_eq!(shell.app().settings().ssh_default_mode, SshMode::Embedded);
    assert_eq!(
        shell.app().profiles()[0].ssh_mode,
        SshMode::External,
        "a reset changes no profile"
    );
}

/// The messages a click on the first "Browse..." of the page sends, once the search holds
/// `query`.
fn browse_clicked(shell: &mut Shell, query: &str) -> Vec<Message> {
    let _ = shell.update(Message::SettingsSearch(query.to_owned()));
    let mut ui = simulator(shell);
    ui.click("Browse...").expect(query);
    ui.into_messages().collect()
}

#[test]
fn every_settings_path_has_a_browse_button_whose_pick_is_applied_at_once() {
    use heimdall_app::ProviderMessage;
    use heimdall_ui::browse::BrowseTarget;
    use heimdall_ui::settings_rows::ToolPath;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::CredentialProvider(
        ProviderMessage::Enabled(true),
    )));
    // Beside each path of the C# page, its "Browse...": found by the row, not searched for.
    for (query, target) in [
        ("putty path", BrowseTarget::Tool(ToolPath::Putty)),
        ("x11 server path", BrowseTarget::Tool(ToolPath::X11Server)),
        ("external editor path", BrowseTarget::ExternalEditor),
        ("session log directory", BrowseTarget::SessionLogDirectory),
        ("database path", BrowseTarget::ProviderDatabase),
        (
            "selfservice.exe path",
            BrowseTarget::Tool(ToolPath::CitrixSelfService),
        ),
        (
            "selfservice cache folder",
            BrowseTarget::Tool(ToolPath::CitrixCacheDirectory),
        ),
    ] {
        let messages = browse_clicked(&mut shell, query);
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::Browse(sent) if *sent == target)),
            "{query}: {messages:?}"
        );
    }
    assert_eq!(shell.settings_found("putty path"), [SettingRow::PuttyPath]);
    // The last rows added, the welcome tour's button, the RDP keep-alive interval, the
    // built-in client's compression, the properties panel's box, the SSH reconnect waits,
    // the RDP disconnect confirmation and redirection indicators, the terminal minimum contrast
    // choice, and the gateway badge's box, hold no path; the external
    // tools' card holds the Browse buttons of the tool chosen, which its own tests cover, and
    // the MobaXterm variables' card holds none; the shared sources' card holds one per source,
    // which its own test covers; the Citrix store prefix is a name, not a path.
    assert_eq!(SettingRow::ALL.len(), 81, "no row added");

    // The path picked is applied as Enter applies what is typed; what was typed goes.
    let _ = shell.update(Message::ToolPathEdited(
        ToolPath::Putty,
        "/typed/putty".to_owned(),
    ));
    for (target, picked) in [
        (BrowseTarget::Tool(ToolPath::Putty), "/opt/putty/putty"),
        (BrowseTarget::Tool(ToolPath::X11Server), "/opt/x/vcxsrv"),
        (
            BrowseTarget::Tool(ToolPath::CitrixSelfService),
            "/opt/citrix/SelfService.exe",
        ),
        (
            BrowseTarget::Tool(ToolPath::CitrixCacheDirectory),
            "/var/cache/citrix",
        ),
        (BrowseTarget::ExternalEditor, "/opt/editor/edit"),
        (BrowseTarget::SessionLogDirectory, "/var/log/heimdall"),
        (BrowseTarget::ProviderDatabase, "/home/me/vault.kdbx"),
        (BrowseTarget::ProviderKeyFile, "/home/me/vault.keyx"),
    ] {
        let _ = shell.update(Message::Browsed(target, picked.to_owned()));
    }
    let _ = shell.update(Message::ToolPathApply(ToolPath::Putty));
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert_eq!(saved.putty_path, "/opt/putty/putty", "the typed text went");
    assert_eq!(saved.x11_server_path, "/opt/x/vcxsrv");
    assert_eq!(
        saved.citrix_self_service_path,
        "/opt/citrix/SelfService.exe"
    );
    assert_eq!(saved.citrix_cache_directory, "/var/cache/citrix");
    assert_eq!(saved.external_editor, "/opt/editor/edit");
    assert_eq!(saved.session_log_directory, "/var/log/heimdall");
    assert_eq!(saved.credential_provider.database, "/home/me/vault.kdbx");
    assert_eq!(saved.credential_provider.key_file, "/home/me/vault.keyx");
}

#[test]
fn a_browse_dialog_picks_a_folder_or_a_file_with_the_csharp_filters_where_the_path_is() {
    use heimdall_ui::browse::BrowseTarget;
    use heimdall_ui::settings_rows::ToolPath;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("putty.exe");
    assert!(BrowseTarget::SessionLogDirectory.picks_folder());
    assert!(BrowseTarget::Tool(ToolPath::CitrixCacheDirectory).picks_folder());
    for target in [
        BrowseTarget::Tool(ToolPath::CitrixSelfService),
        BrowseTarget::Tool(ToolPath::Putty),
        BrowseTarget::Tool(ToolPath::X11Server),
        BrowseTarget::ExternalEditor,
        BrowseTarget::ProviderDatabase,
        BrowseTarget::ProviderKeyFile,
        BrowseTarget::GatewayKey,
    ] {
        assert!(!target.picks_folder(), "{target:?}");
        assert_eq!(
            target.start(&file).as_deref(),
            Some(dir.path()),
            "a file's folder"
        );
    }
    assert_eq!(
        BrowseTarget::SessionLogDirectory
            .start(dir.path())
            .as_deref(),
        Some(dir.path()),
        "a folder itself"
    );
    assert_eq!(
        BrowseTarget::ExternalEditor.start(&dir.path().join("gone/edit.exe")),
        None,
        "a folder that does not exist: the system's own"
    );
    assert_eq!(
        BrowseTarget::Tool(ToolPath::Putty).title().as_deref(),
        Some("Select putty.exe or puttycac.exe")
    );
    assert_eq!(
        BrowseTarget::SessionLogDirectory.title().as_deref(),
        Some("Select Session Log Directory")
    );
    let names = |target: BrowseTarget| -> Vec<(String, Vec<&str>)> {
        target
            .filters()
            .into_iter()
            .map(|(name, extensions)| (name, extensions.to_vec()))
            .collect()
    };
    assert_eq!(
        names(BrowseTarget::ProviderDatabase),
        [
            ("Database files".to_owned(), vec!["kdbx", "db", "gpg"]),
            ("All files".to_owned(), vec!["*"]),
        ]
    );
    assert_eq!(
        names(BrowseTarget::ProviderKeyFile),
        [
            ("Key files".to_owned(), vec!["keyx", "key"]),
            ("All files".to_owned(), vec!["*"]),
        ]
    );
    assert_eq!(
        names(BrowseTarget::GatewayKey),
        [
            ("PPK files".to_owned(), vec!["ppk"]),
            ("PEM files".to_owned(), vec!["pem"]),
            ("All files".to_owned(), vec!["*"]),
        ]
    );
    let programs = names(BrowseTarget::Tool(ToolPath::Putty));
    if cfg!(windows) {
        assert_eq!(
            programs,
            [
                ("Executables".to_owned(), vec!["exe"]),
                ("All files".to_owned(), vec!["*"]),
            ]
        );
    } else {
        assert!(programs.is_empty(), "a program has no extension here");
    }
}

#[test]
fn reset_defaults_asks_first_keeps_the_theme_and_forgets_what_is_typed() {
    use heimdall_app::Dialog;
    use heimdall_ui::settings_rows::ToolPath;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    change(&mut shell, SettingsMessage::Theme(AppTheme::Tarn));
    change(&mut shell, SettingsMessage::ColorScheme(ColorScheme::Nord));
    let _ = shell.update(Message::ToolPathEdited(
        ToolPath::Putty,
        "/typed/putty".to_owned(),
    ));
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.click("Reset defaults")
            .expect("the C# button, above the tabs");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::ConfirmResetAllSettings)
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("Reset all settings?").expect("the C# question");
        ui.find(RESET_ALL_BODY).expect("what it does and keeps");
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let settings = shell.app().settings();
    assert_eq!(settings.color_scheme, Settings::default().color_scheme);
    assert_eq!(settings.theme, AppTheme::Tarn, "kept, as the C#");
    // What was typed over the old value went with it.
    let _ = shell.update(Message::ToolPathApply(ToolPath::Putty));
    assert!(shell.app().settings().putty_path.is_empty());
    let modified = shell.settings_found("modified");
    assert_eq!(modified, [SettingRow::Theme], "only what the reset keeps");
}

/// What "Reset defaults" asks.
const RESET_ALL_BODY: &str = "Restore every Settings tab to factory defaults? Text typed and \
    not applied yet is discarded. On the Security tab this turns off the external credential \
    provider, the Credential Guard requirement and the Windows Hello requirement on connect, \
    and resets the Windows Hello grace \
    period, the auto-lock delay and disconnect on lock. Your language, theme, sessions, SSH \
    gateways, master password, PIN and Windows Hello enrolment are kept. The change is saved \
    at once.";

#[test]
fn a_skipped_release_is_said_on_the_updates_card_and_offered_again() {
    use heimdall_app::UpdateMessage;
    use heimdall_core::settings::UpdateCheck;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    Settings {
        update_check: UpdateCheck {
            last_check: None,
            skipped: Some("v2026.101001".to_owned()),
        },
        ..Settings::default()
    }
    .save(&settings_path(&dir.path().join("profiles.toml")))
    .expect("saved");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(SettingsSection::Updates));
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.find("Skipped version: 2026.101001")
            .expect("as the C# panel says it");
        ui.click("Offer it again").expect("its button");
        ui.into_messages().collect()
    };
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::App(AppMessage::Update(UpdateMessage::ClearSkipped))
    )));
    for message in messages {
        let _ = shell.update(message);
    }
    assert_eq!(shell.app().settings().update_check.skipped, None);
    let mut ui = simulator(&shell);
    assert!(ui.find("Offer it again").is_err(), "nothing skipped");
}

#[test]
fn hardware_acceleration_is_said_not_supported_yet_among_the_rdp_defaults() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSearch(
        "hardware-accelerated rendering".to_owned(),
    ));
    let mut ui = simulator(&shell);
    let tick = ui.find("Hardware-accelerated rendering").expect("the box");
    let said = ui.find(HARDWARE_UNSUPPORTED).expect("said under it");
    assert!(said.bounds().y > tick.bounds().y, "under the box");
}

/// What is said of hardware acceleration.
const HARDWARE_UNSUPPORTED: &str = "Not supported yet: the built-in client has no such switch, \
    and Remote Desktop Connection (mstsc.exe) reads no setting for it from its .rdp file.";

#[test]
fn the_default_rdp_mode_is_chosen_found_reset_and_applied_to_all_once_asked() {
    use heimdall_core::profile::{ProfileId, RdpMode, RdpProfile};
    use heimdall_core::store::ProfileStore;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_rdp([RdpProfile {
        remote_os: heimdall_core::profile::RemoteOs::default(),
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "dc".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }]);
    store.save().expect("save");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("default rdp mode"),
        [SettingRow::RdpDefaultMode]
    );
    assert_eq!(
        shell.settings_found("apply to all"),
        [SettingRow::SshDefaultMode, SettingRow::RdpDefaultMode],
        "each protocol's button"
    );
    assert!(
        shell
            .settings_found("mstsc.exe")
            .contains(&SettingRow::RdpDefaultMode),
        "by what is said of it"
    );
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Rdp),
    ));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Default RDP mode",
            "Embedded: RDP client runs inside Heimdall. External: opens mstsc.exe in a separate \
             window.",
            "Apply to all saved sessions",
        ] {
            ui.find(said).expect(said);
        }
        assert!(ui.find("Modified").is_err(), "the default");
    }

    change(
        &mut shell,
        SettingsMessage::RdpDefaultMode(RdpMode::External),
    );
    assert_eq!(
        shell.settings_found("modified"),
        [SettingRow::RdpDefaultMode]
    );
    {
        let mut ui = simulator(&shell);
        ui.click("Apply to all saved sessions").expect("the button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::ApplyRdpModeToAll))
        )));
    }
    change(&mut shell, SettingsMessage::ApplyRdpModeToAll);
    {
        let mut ui = simulator(&shell);
        for said in [
            "Apply to all saved RDP sessions?",
            "1 of 1 saved RDP sessions will switch to the External mode, and External becomes \
             the default for new sessions. This cannot be undone.",
        ] {
            ui.find(said).expect(said);
        }
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().rdp_profiles()[0].extras.external);

    let _ = shell.update(Message::ResetSetting(SettingRow::RdpDefaultMode));
    assert_eq!(shell.app().settings().rdp_default_mode, RdpMode::Embedded);
    assert!(
        shell.app().rdp_profiles()[0].extras.external,
        "a reset changes no profile"
    );
}

#[test]
fn credential_guard_is_required_found_marked_its_state_said_and_reset_on_the_security_tab() {
    use heimdall_app::credential_guard::{Failure, Status};

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("credential guard"),
        [SettingRow::RequireCredentialGuard]
    );
    let _ = shell.update(Message::SettingsSection(SettingsSection::ConnectionChecks));
    simulator(&shell)
        .find("Require Credential Guard")
        .expect("the C# label");
    let _ = shell.update(Message::SettingsSection(SettingsSection::Overview));
    {
        let mut ui = simulator(&shell);
        ui.find("Credential Guard: Not required")
            .expect("the overview line");
        assert!(
            ui.find("Credential Guard: Enabled").is_err(),
            "no state said while not required"
        );
    }
    change(&mut shell, SettingsMessage::RequireCredentialGuard(true));
    assert!(shell.app().settings().require_credential_guard);
    assert_eq!(
        shell.settings_found("modified"),
        [SettingRow::RequireCredentialGuard]
    );
    {
        let mut ui = simulator(&shell);
        ui.find("Credential Guard: Required")
            .expect("the overview line");
        assert!(
            ui.find("Credential Guard: Not available").is_err(),
            "unknown yet"
        );
    }
    // Where PowerShell is kept from starting, the reason is said for the administrator, on
    // the card of the check.
    let _ = shell.update(Message::SettingsSection(SettingsSection::ConnectionChecks));
    let _ = shell.update(Message::App(AppMessage::CredentialGuard(
        Status::Indeterminate(Failure::NotStarted("blocked by group policy".to_owned())),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find(
            "Credential Guard: Not available (PowerShell could not start: blocked by group \
             policy)",
        )
        .expect("the reason");
    }
    let _ = shell.update(Message::App(AppMessage::CredentialGuard(Status::Active)));
    {
        let mut ui = simulator(&shell);
        ui.find("Credential Guard: Enabled").expect("the C# status");
    }
    let _ = shell.update(Message::ResetSetting(SettingRow::RequireCredentialGuard));
    assert!(!shell.app().settings().require_credential_guard);
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert!(!saved.require_credential_guard);
}

#[test]
fn the_known_hosts_import_is_found_marked_flagged_in_the_overview_and_reset() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("known_hosts file at startup"),
        [SettingRow::SyncKnownHostsAtStartup]
    );
    let _ = shell.update(Message::SettingsSection(SettingsSection::SshHostKeys));
    {
        let mut ui = simulator(&shell);
        ui.find("Import the OpenSSH known_hosts file at startup")
            .expect("the C# label");
        ui.find(
            "Each time Heimdall starts, host keys from your user's .ssh/known_hosts are added \
             to this list. A key that differs from one already trusted here is not replaced.",
        )
        .expect("the C# hint");
    }
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("known_hosts import at startup: Off")
            .expect("the overview line");
    }
    change(&mut shell, SettingsMessage::SyncKnownHostsAtStartup(true));
    assert!(shell.app().settings().sync_known_hosts_at_startup);
    assert_eq!(
        shell.settings_found("modified"),
        [SettingRow::SyncKnownHostsAtStartup]
    );
    {
        let mut ui = simulator(&shell);
        ui.find("known_hosts import at startup: On")
            .expect("the overview line");
        ui.find(
            "At every start, host keys from known_hosts, a file other programs also write, \
             are trusted without asking.",
        )
        .expect("the C# warning");
    }
    let _ = shell.update(Message::ResetSetting(SettingRow::SyncKnownHostsAtStartup));
    assert!(!shell.app().settings().sync_known_hosts_at_startup);
    let saved = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("saved");
    assert!(!saved.sync_known_hosts_at_startup);
}

/// The C# "Legacy migration" of the Updates card: its button can be pressed only once an
/// offer was declined, which the hint under it says until then.
#[test]
fn the_legacy_migration_is_offered_again_from_the_updates_card_once_declined() {
    use heimdall_app::LegacyMigrationMessage;
    use heimdall_core::settings::LegacyMigration;

    const BUTTON: &str = "Offer legacy migration at next startup";
    const HINT: &str =
        "This becomes available only after you decline a migration offer at startup.";
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shown = shell(dir.path());
    let _ = shown.update(Message::SettingsSection(SettingsSection::Updates));
    assert_eq!(
        shown.settings_found("legacy migration"),
        [SettingRow::LegacyMigration]
    );
    assert_eq!(SettingRow::LegacyMigration.card(), SettingsCard::Updates);
    {
        let mut ui = simulator(&shown);
        ui.find(HINT).expect("the hint");
        ui.click(BUTTON).expect("the button");
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::LegacyMigration(_)))),
            "nothing declined: nothing to offer again"
        );
    }

    let declined = Settings {
        legacy_migration: LegacyMigration {
            declined_offer_version: 1,
            declined_source_fingerprint: Some("ABCD".to_owned()),
        },
        ..Settings::default()
    };
    declined
        .save(&settings_path(&dir.path().join("profiles.toml")))
        .expect("saved");
    let mut shown = shell(dir.path());
    let _ = shown.update(Message::SettingsSection(SettingsSection::Updates));
    let mut ui = simulator(&shown);
    assert!(ui.find(HINT).is_err(), "available");
    ui.click(BUTTON).expect("the button");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::LegacyMigration(
            LegacyMigrationMessage::OfferAgain
        ))
    )));
}

#[test]
fn an_external_tool_is_added_from_its_card_and_applied_once_its_program_is_found() {
    use heimdall_ui::browse::BrowseTarget;
    use heimdall_ui::external_tools_view::ToolsEdit;
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("external tools"),
        [SettingRow::ExternalTools]
    );
    let _ = shell.update(Message::ExternalToolsEdit(ToolsEdit::Add));
    assert!(
        shell.app().settings().external_tools.is_empty(),
        "no program yet: kept in the card"
    );
    let program = std::env::current_exe()
        .expect("the test itself")
        .to_string_lossy()
        .into_owned();
    let _ = shell.update(Message::Browsed(
        BrowseTarget::ExternalToolProgram,
        program.clone(),
    ));
    let tools = &shell.app().settings().external_tools;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "New Tool");
    assert_eq!(tools[0].executable_path, program);
    assert_eq!(tools[0].arguments, "{Host}");
}

#[test]
fn a_mobaxterm_variable_is_added_from_its_card_and_applied_once_it_has_a_name_and_a_value() {
    use heimdall_ui::mobaxterm_variables_view::VariablesEdit;
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("import variables"),
        [SettingRow::MobaxtermVariables]
    );
    let _ = shell.update(Message::MobaxtermVariablesEdit(VariablesEdit::Add));
    assert_eq!(
        shell.app().settings().mobaxterm_variables.len(),
        1,
        "the empty row waits in the card"
    );
    let _ = shell.update(Message::MobaxtermVariablesEdit(VariablesEdit::Name(
        1,
        "TEAM".to_owned(),
    )));
    let _ = shell.update(Message::MobaxtermVariablesEdit(VariablesEdit::Value(
        1,
        "ops".to_owned(),
    )));
    assert_eq!(
        shell.app().settings().mobaxterm_variables.get("TEAM"),
        Some(&"ops".to_owned())
    );
    let _ = shell.update(Message::MobaxtermVariablesEdit(VariablesEdit::Remove(0)));
    let _ = shell.update(Message::MobaxtermVariablesEdit(VariablesEdit::Remove(0)));
    assert!(shell.app().settings().mobaxterm_variables.is_empty());
}

#[test]
fn an_unresolved_mobaxterm_variable_is_a_line_of_the_import_report() {
    use heimdall_core::import::foreign::FileWarning;
    let _english = english();
    let lines = heimdall_ui::file_import_view::warning_lines(&[FileWarning::UnresolvedVariable {
        session: "db".to_owned(),
        name: "GHOST".to_owned(),
    }]);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("has no value") && lines[0].contains("GHOST") && lines[0].contains("db"),
        "{}",
        lines[0]
    );
}

#[test]
fn every_mobaxterm_session_left_out_says_why_in_the_import_report() {
    use heimdall_core::import::foreign::FileWarning;
    let _english = english();
    let session = || "box".to_owned();
    let folder = || "Prod".to_owned();
    let lines = heimdall_ui::file_import_view::warning_lines(&[
        FileWarning::WslNotSupported {
            session: session(),
            folder: folder(),
        },
        FileWarning::UnsupportedSessionType {
            session: session(),
            folder: folder(),
            type_code: "77".to_owned(),
        },
        FileWarning::JumpHostNotSupported {
            session: session(),
            folder: folder(),
            host: "jump.lab".to_owned(),
        },
        FileWarning::ProxyNotSupported {
            session: session(),
            folder: folder(),
            host: "proxy.lab".to_owned(),
        },
        FileWarning::ProxyNotSupported {
            session: session(),
            folder: folder(),
            host: String::new(),
        },
    ]);
    assert_eq!(lines.len(), 5);
    assert!(lines[0].contains("WSL") && lines[0].contains("nothing to connect"));
    assert!(lines[1].contains("77") && lines[1].contains("not supported"));
    assert!(lines[2].contains("jump.lab") && lines[2].contains("jump host"));
    assert!(lines[3].contains("proxy.lab"));
    assert!(lines[4].contains("a proxy"));
    assert!(lines.iter().all(|line| line.contains("not imported")));
}

#[test]
fn a_shared_source_is_added_from_its_card_picked_and_applied_once_it_passes_the_row_checks() {
    use heimdall_ui::browse::BrowseTarget;
    use heimdall_ui::shared_sources_view::SourcesEdit;
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert_eq!(
        shell.settings_found("shared session sources"),
        [SettingRow::SharedSources]
    );
    let _ = shell.update(Message::SharedSourcesEdit(SourcesEdit::Add));
    assert!(
        shell.app().settings().shared_sources.is_empty(),
        "the empty row waits in the card"
    );
    let _ = shell.update(Message::SharedSourcesEdit(SourcesEdit::Name(
        0,
        "Team".to_owned(),
    )));
    // Picked with Browse, as the typed path is applied.
    let file = dir.path().join("team.mxtsessions");
    std::fs::write(
        &file,
        "[Bookmarks]
web= #109#0%web.lab%22%ops%
",
    )
    .expect("write");
    let picked = file.to_string_lossy().into_owned();
    let _ = shell.update(Message::Browsed(
        BrowseTarget::SharedSourcePath(0),
        picked.clone(),
    ));
    let sources = &shell.app().settings().shared_sources;
    assert_eq!(sources.len(), 1);
    assert_eq!(
        (sources[0].name.as_str(), sources[0].path.as_str()),
        ("Team", picked.as_str())
    );
    assert_eq!(sources[0].id.len(), 32, "an identifier of its own");
    assert_eq!(
        shell.app().shared_sessions().sessions().count(),
        1,
        "read at once"
    );
    // The report opens and closes without changing the list.
    let _ = shell.update(Message::SharedSourcesEdit(SourcesEdit::Report(0)));
    let _ = shell.update(Message::SharedReportClose);
    assert_eq!(shell.app().settings().shared_sources.len(), 1);
    // A relative path is refused as the import refuses it, and the list stays as it was.
    let _ = shell.update(Message::SharedSourcesEdit(SourcesEdit::Path(
        0,
        "relative.mxtsessions".to_owned(),
    )));
    assert_eq!(shell.app().settings().shared_sources[0].path, picked);
    let _ = shell.update(Message::SharedSourcesEdit(SourcesEdit::Remove(0)));
    assert!(shell.app().settings().shared_sources.is_empty());
}

#[test]
fn a_refused_citrix_value_is_said_under_its_field_and_the_old_one_kept() {
    use heimdall_ui::settings_rows::ToolPath;

    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let apply = |shell: &mut Shell, path: ToolPath, text: &str| {
        let _ = shell.update(Message::ToolPathEdited(path, text.to_owned()));
        let _ = shell.update(Message::ToolPathApply(path));
    };
    let saved = |dir: &std::path::Path| {
        Settings::load(&settings_path(&dir.join("profiles.toml"))).expect("saved")
    };

    apply(
        &mut shell,
        ToolPath::CitrixCacheDirectory,
        "/var/cache/citrix",
    );
    assert!(!shell.tool_path_refused(ToolPath::CitrixCacheDirectory));
    for (path, bad) in [
        (ToolPath::CitrixCacheDirectory, r"/\host\share"),
        (ToolPath::CitrixCacheDirectory, r"%LOCALAPPDATA%\x"),
        (ToolPath::CitrixSelfService, "/opt/citrix/other.exe"),
        (ToolPath::CitrixStorePrefix, "a/b"),
    ] {
        apply(&mut shell, path, bad);
        assert!(shell.tool_path_refused(path), "{bad}");
    }
    let kept = saved(dir.path());
    assert_eq!(kept.citrix_cache_directory, "/var/cache/citrix");
    assert!(kept.citrix_self_service_path.is_empty());
    assert!(kept.citrix_cache_store_prefix.is_empty());

    // Typing again takes the message away; a valid value is applied.
    let _ = shell.update(Message::ToolPathEdited(
        ToolPath::CitrixStorePrefix,
        "Corp".to_owned(),
    ));
    assert!(!shell.tool_path_refused(ToolPath::CitrixStorePrefix));
    let _ = shell.update(Message::ToolPathApply(ToolPath::CitrixStorePrefix));
    assert_eq!(saved(dir.path()).citrix_cache_store_prefix, "Corp");
}

/// Whether the page `shell` draws says `said`.
fn shows(shell: &Shell, said: &str) -> bool {
    simulator(shell).find(said).is_ok()
}

#[test]
fn one_rail_lists_every_section_under_its_group_and_opens_one_alone() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    {
        let mut ui = simulator(&shell);
        // The six group names are headings, and every section of every group is listed.
        for tab in ["GENERAL", "TERMINAL", "SSH", "RDP", "SECURITY"] {
            ui.find(tab).expect(tab);
        }
        for name in [
            "Appearance",
            "Updates",
            "Welcome tour",
            "Host keys",
            "Display & Audio",
            "Certificates",
            "Overview",
            "Master password & PIN",
            "Credential provider",
            "Connection checks",
        ] {
            ui.find(name).expect(name);
        }
        ui.find("No risky setting")
            .expect("the overview, the first section of Security");
        assert!(
            ui.find("Require Credential Guard").is_err(),
            "another section's"
        );
        ui.click("Connection checks")
            .expect("its section on the rail");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::SettingsSection(SettingsSection::ConnectionChecks)
        )));
    }
    let _ = shell.update(Message::SettingsSection(SettingsSection::ConnectionChecks));
    assert!(shows(&shell, "Require Credential Guard"));
    assert!(!shows(&shell, "No risky setting"), "the overview gave way");
    // A group's name opens its first section.
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::General),
    ));
    assert!(shows(&shell, "Language"), "General opens on Appearance");
    assert!(!shows(&shell, "Check for updates automatically"));
    assert!(!shows(&shell, "Show the tour again"));
}

#[test]
fn the_glyph_is_explained_once_at_the_foot_of_the_rail_and_every_setting_applies_as_changed() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(SettingsSection::Terminal));
    let mut ui = simulator(&shell);
    for said in [
        "Settings apply as you change them",
        "Applies to connections opened after the change",
    ] {
        ui.find(said).expect(said);
    }
    // Nothing says "Save" or "saved immediately": there is no Save here.
    assert!(ui.find("Saved immediately").is_err());
    assert!(ui.find("Applies immediately").is_err());
    // The glyph is per setting: the sessions' wording is in its tooltip, not on the page.
    assert!(
        ui.find("Applies to sessions opened after the change")
            .is_err()
    );
}

#[test]
fn home_and_end_open_the_first_and_the_last_section_once_the_rail_is_clicked() {
    use heimdall_app::files::FilesKey;
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(SettingsSection::Provider));
    let _ = shell.update(Message::FilesKey(FilesKey::First));
    assert!(shows(&shell, "Language"), "Appearance, the first");
    let _ = shell.update(Message::FilesKey(FilesKey::Last));
    assert!(shows(&shell, "Require Credential Guard"), "the last");
}

#[test]
fn the_longest_french_and_spanish_section_names_stay_on_one_line_in_the_rail() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    for language in [Language::French, Language::Spanish] {
        heimdall_ui::i18n::apply(Some(language));
        let mut shell = shell(dir.path());
        for section in SettingsSection::ALL {
            // Open, so its name is the semi-bold one, the widest.
            let _ = shell.update(Message::SettingsSection(section));
            let mut ui = simulator(&shell);
            let name = ui.find(section_name(section, language)).expect("its name");
            // One line of the body text is well under 24 px; two lines are over 30.
            assert!(
                name.bounds().height < 24.0,
                "{language:?} {section:?}: {:?}",
                name.bounds()
            );
        }
    }
}

/// The name of `section` on the rail in `language`, as the catalogue says it.
fn section_name(section: SettingsSection, language: Language) -> &'static str {
    use SettingsSection as S;
    let (french, spanish) = match section {
        S::Appearance => ("Apparence", "Apariencia"),
        S::Behavior | S::RdpBehavior => ("Comportement", "Comportamiento"),
        S::Updates => ("Mises à jour", "Actualizaciones"),
        S::Import => ("Fichier et import", "Archivo e importación"),
        S::SharedSources => ("Sources partagées", "Orígenes compartidos"),
        S::Tools => ("Outils et intégrations", "Herramientas e integraciones"),
        S::Onboarding => ("Visite guidée", "Recorrido de bienvenida"),
        S::Terminal => ("Terminal", "Terminal"),
        S::SshConnection => ("Connexion", "Conexión"),
        S::SshSession => ("Session", "Sesión"),
        S::SftpX11 => ("SFTP et X11", "SFTP y X11"),
        S::SshHostKeys => ("Clés d'hôtes", "Claves de host"),
        S::Gateways => ("Passerelles", "Pasarelas"),
        S::RdpDisplay => ("Affichage et audio", "Pantalla y audio"),
        S::RdpCertificates => ("Certificats", "Certificados"),
        S::Overview => ("Vue d'ensemble", "Resumen"),
        S::Vault => ("Mot de passe maître et PIN", "Contraseña maestra y PIN"),
        S::Provider => ("Fournisseur d'identifiants", "Proveedor de credenciales"),
        S::ConnectionChecks => ("Contrôles à la connexion", "Comprobaciones al conectar"),
    };
    if language == Language::French {
        french
    } else {
        spanish
    }
}

#[test]
fn the_rdp_reset_shows_under_each_rdp_section_and_under_no_other() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Rdp),
    ));
    assert!(shows(&shell, "Default RDP mode"), "Display & Audio first");
    for section in SettingsSection::ALL {
        let _ = shell.update(Message::SettingsSection(section));
        assert_eq!(
            shows(&shell, "Reset RDP defaults"),
            section.tab() == SettingsTab::Rdp,
            "{section:?}"
        );
    }
}

#[test]
fn go_to_setting_opens_the_section_that_shows_the_row() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    let _ = shell.update(Message::GoToSetting(SettingRow::RequireCredentialGuard));
    assert!(shows(&shell, "Require Credential Guard"));
    // From another group, into a section that is not its group's first.
    let _ = shell.update(Message::GoToSetting(SettingRow::UpdateChecks));
    assert!(shows(&shell, "Check for updates automatically"));
    assert!(!shows(&shell, "Language"), "Appearance gave way");
    let _ = shell.update(Message::GoToSetting(SettingRow::PuttyPath));
    assert!(shows(&shell, "PuTTY path (for External SSH mode)"));
}

#[test]
fn up_and_down_move_along_the_one_rail_once_clicked_and_round() {
    use heimdall_app::files::FilesKey;
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::General),
    ));
    // Before the rail is clicked, a field or a list may have the keys: they move nothing.
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(
        shows(&shell, "Language"),
        "the rail does not have the keyboard"
    );
    let _ = shell.update(Message::SettingsSection(SettingsSection::Appearance));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(
        shows(&shell, "Enable background reachability probes"),
        "Behavior, below Appearance"
    );
    assert!(!shows(&shell, "Language"));
    let _ = shell.update(Message::FilesKey(FilesKey::Previous));
    assert!(shows(&shell, "Language"), "and back up");
    // Up from the first goes round to the last section of the last group.
    let _ = shell.update(Message::FilesKey(FilesKey::Previous));
    assert!(
        shows(&shell, "Require Credential Guard"),
        "Connection checks, as the C# rail cycles"
    );
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(shows(&shell, "Language"), "down from the last, the first");
    // Down from the last section of a group crosses the next group's name, which is no stop.
    let _ = shell.update(Message::SettingsSection(SettingsSection::Onboarding));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(
        shows(&shell, "Session Logging"),
        "the Terminal group's only section"
    );
    assert!(!shows(&shell, "Show the tour again"));
    // A press elsewhere, a field or a list, takes the keyboard from the rail.
    let _ = shell.update(Message::SettingsSection(SettingsSection::Appearance));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(shows(&shell, "Language"), "a press elsewhere");
    // So does Tab.
    let _ = shell.update(Message::SettingsSection(SettingsSection::Appearance));
    let _ = shell.update(Message::TabKey { backward: false });
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert!(shows(&shell, "Language"), "Tab leaves the rail");
    // A search shown has no section open: the keys leave the sections alone.
    let _ = shell.update(Message::SettingsSection(SettingsSection::Appearance));
    let _ = shell.update(Message::SettingsSearch("tour".to_owned()));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    let _ = shell.update(Message::SettingsSearch(String::new()));
    assert!(shows(&shell, "Language"));
}

#[test]
fn the_search_finds_a_row_of_a_section_that_is_not_open() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSection(
        heimdall_ui::settings_rows::SettingsSection::first_of(SettingsTab::Security),
    ));
    assert!(!shows(&shell, "Require Credential Guard"));
    let _ = shell.update(Message::SettingsSearch("credential guard".to_owned()));
    assert!(shows(&shell, "Require Credential Guard"));
}

#[test]
fn the_appearance_card_lays_its_three_lists_side_by_side_as_the_csharps_grid() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    // The caption of each list: the rail and the status bar say "Theme" or "Language" in
    // no other place of the page's body, so the last text is the card's.
    let language = common::last_bounds(&mut ui, "Language");
    let theme = common::last_bounds(&mut ui, "Theme");
    let accent = common::last_bounds(&mut ui, "Accent");
    assert!(
        (language.y - theme.y).abs() < 1.0 && (theme.y - accent.y).abs() < 1.0,
        "one line: {language:?} {theme:?} {accent:?}"
    );
    assert!(
        language.x < theme.x && theme.x < accent.x,
        "language, theme, accent from the left: {language:?} {theme:?} {accent:?}"
    );
    // Three columns of the same width: the gaps between their left edges are equal.
    let first = theme.x - language.x;
    let second = accent.x - theme.x;
    assert!((first - second).abs() < 1.0, "{first} against {second}");
}

#[test]
fn the_gateway_badge_is_ticked_in_the_session_tree_card_and_is_the_filter_menus_setting() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    assert!(
        shell.app().shows_gateway_badge(),
        "shown, as the C# default"
    );
    assert_eq!(
        SettingRow::ShowGatewayBadge.card(),
        SettingsCard::SessionTree
    );
    assert_eq!(
        shell.settings_found("gateway badge"),
        [SettingRow::ShowGatewayBadge],
        "found by its name"
    );
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.click("Show gateway badge").expect("the box");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
    assert!(!shell.app().shows_gateway_badge(), "off, as the menu's");
    assert!(
        SettingRow::ShowGatewayBadge.is_modified(shell.app().settings()),
        "marked once off its default"
    );
    let _ = shell.update(Message::ResetSetting(SettingRow::ShowGatewayBadge));
    assert!(shell.app().shows_gateway_badge(), "reset");
}

#[test]
fn the_rail_names_the_files_section_as_the_csharp_does_and_its_legend_names_both_glyph_meanings() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    for said in [
        "Settings file & import",
        "Settings apply as you change them",
        "Applies to connections opened after the change",
    ] {
        ui.find(said).expect(said);
    }
    assert!(ui.find("Import").is_err(), "no longer the bare word");
}

#[test]
fn the_gateway_badge_box_says_it_is_also_in_the_tree_filter_menu() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    ui.find("Shows on each session row the gateway it goes through. Also in the tree filter menu.")
        .expect("the hint");
}

#[test]
fn the_appearance_card_stacks_its_lists_in_a_narrow_window_with_the_markers_in_spanish() {
    let _english = english();
    heimdall_ui::i18n::apply(Some(Language::Spanish));
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    change(&mut shell, SettingsMessage::Theme(AppTheme::Parchment));
    change(&mut shell, SettingsMessage::Accent(Accent::Blue));
    // The smallest window, the main sidebar open.
    let narrow = Size::new(800.0, 600.0);
    let _ = shell.update(Message::WindowResized(narrow));
    let settings = IcedSettings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..IcedSettings::default()
    };
    let mut ui = common::simulator(settings, narrow, shell.view());
    let language = common::last_bounds(&mut ui, "Idioma");
    let theme = common::last_bounds(&mut ui, "Tema");
    let accent = common::last_bounds(&mut ui, "Acento");
    assert!(
        language.y < theme.y && theme.y < accent.y,
        "one under the other: {language:?} {theme:?} {accent:?}"
    );
    assert!(
        (language.x - theme.x).abs() < 1.0 && (theme.x - accent.x).abs() < 1.0,
        "one column: {language:?} {theme:?} {accent:?}"
    );
    // Neither marker leaves the window.
    for marker in common::all_bounds(&mut ui, "Restablecer") {
        assert!(marker.x + marker.width <= narrow.width, "{marker:?}");
    }
}

#[test]
fn a_search_leaving_one_row_of_the_appearance_card_shows_it_alone_not_in_columns() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::SettingsSearch("accent".to_owned()));
    assert_eq!(shell.settings_found("accent"), [SettingRow::Accent]);
    let mut ui = simulator(&shell);
    let accent = common::last_bounds(&mut ui, "Accent");
    // The rail names "Appearance" too: the card's heading is the last.
    let card = common::last_bounds(&mut ui, "Appearance");
    assert!(
        (accent.x - card.x).abs() < 1.0,
        "under the heading at the card's left edge, not in a third column: {accent:?} {card:?}"
    );
}
