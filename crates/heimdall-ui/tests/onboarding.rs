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

//! The welcome tour, as the C# onboarding overlay: shown at the first start only, stepped
//! through with Next or Enter, ended with Skip, Escape or Get Started, recorded as seen, and
//! shown again from the Settings page's "Show the tour again".

mod common;

use std::path::Path;

use heimdall_app::tools::SidebarTab;
use heimdall_app::{App, AppConfig, Message as AppMessage, SystemCredentials, ToolsMessage};
use heimdall_core::settings::{Language, Settings, settings_path};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::onboarding::{TourMessage, TourStep};
use heimdall_ui::settings_rows::{SettingRow, SettingsCard};
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings as IcedSettings, Size};

/// A window as large as the C# one opens.
const WINDOW: Size = Size::new(1280.0, 800.0);

/// A window tall enough for the whole General tab of the Settings page.
const SETTINGS_WINDOW: Size = Size::new(1100.0, 2400.0);

/// The first step's heading and the last's, as the C# `OnboardingStep1Title` and
/// `OnboardingStepSettingsTitle`.
const FIRST_TITLE: &str = "Connect to Sessions";
const LAST_TITLE: &str = "Settings, and this tour";

/// The card's buttons, as the C# `OnboardingBtn*`.
const SKIP: &str = "Skip";
const NEXT: &str = "Next";
const GET_STARTED: &str = "Get Started";

/// The Settings page's button, as the C# `SettingsOnboardingReplayButton`.
const REPLAY: &str = "Show the tour again";

/// What the card says when its end cannot be saved, as the C#
/// `OnboardingCompletionSaveFailed`.
const SAVE_FAILED: &str = "Heimdall couldn't save setup completion. Check access to the configuration folder, then try again.";

fn app(dir: &Path) -> App {
    heimdall_ui::i18n::apply(Some(Language::English));
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

/// The window of a start over `dir`, as `Shell::new` makes it: the tour offered.
fn started(dir: &Path) -> Shell {
    let mut shell = Shell::with_app(app(dir));
    shell.offer_tour_on_first_run();
    shell
}

fn simulator_at(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = IcedSettings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..IcedSettings::default()
    };
    common::simulator(settings, size, shell.view())
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    simulator_at(shell, WINDOW)
}

/// Clicks `label` on the window of `shell`, then applies what the click sent.
fn click(shell: &mut Shell, label: &str) {
    let messages: Vec<Message> = {
        let mut ui = simulator(shell);
        ui.click(label).expect(label);
        ui.into_messages().collect()
    };
    assert!(!messages.is_empty(), "{label}: nothing sent");
    for message in messages {
        let _ = shell.update(message);
    }
}

/// Whether the settings file in `dir` says the tour was seen.
fn recorded(dir: &Path) -> bool {
    Settings::load(&settings_path(&dir.join("profiles.toml")))
        .expect("settings")
        .onboarding_completed
}

#[test]
fn the_tour_shows_on_a_first_start_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = started(dir.path());
    assert_eq!(
        shell.tour().map(heimdall_ui::onboarding::Tour::step),
        Some(TourStep::Sessions)
    );
    {
        let mut ui = simulator(&shell);
        ui.find(FIRST_TITLE).expect("the first step");
        ui.find(SKIP).expect("Skip");
        ui.find(NEXT).expect("Next");
    }
    assert!(!recorded(dir.path()), "nothing recorded while it shows");
    click(&mut shell, SKIP);
    assert!(shell.tour().is_none(), "skipped");
    assert!(recorded(dir.path()), "Skip records it, as the C# SkipAsync");
    assert!(simulator(&shell).find(FIRST_TITLE).is_err());

    // The next start: not again.
    let shell = started(dir.path());
    assert!(shell.tour().is_none());
    assert!(simulator(&shell).find(FIRST_TITLE).is_err());

    // A window made without a start, as the other tests make theirs, shows none either.
    let fresh = tempfile::tempdir().expect("dir");
    assert!(Shell::with_app(app(fresh.path())).tour().is_none());
}

#[test]
fn next_steps_through_every_c_sharp_step_and_get_started_ends_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = started(dir.path());
    // The tools' sidebar shown: the end of the tour shows the sessions' again.
    let _ = shell.update(Message::App(AppMessage::Tools(
        ToolsMessage::ShowSidebarTab(SidebarTab::Tools),
    )));
    for step in &TourStep::ALL[..TourStep::ALL.len() - 1] {
        let title = step.title();
        {
            let mut ui = simulator(&shell);
            ui.find(title.as_str()).expect(&title);
            assert!(ui.find(GET_STARTED).is_err(), "{step:?}: not the last yet");
        }
        click(&mut shell, NEXT);
        assert!(!recorded(dir.path()), "{step:?}: Next is not the end");
    }
    {
        let mut ui = simulator(&shell);
        ui.find(LAST_TITLE).expect("the last step");
        assert!(ui.find(NEXT).is_err(), "Get Started in its place");
    }
    // Opened from the Settings page, the steps about the sessions show their page first.
    let _ = shell.update(Message::ShowSettings);
    assert!(shell.settings_shown());
    click(&mut shell, GET_STARTED);
    assert!(shell.tour().is_none(), "finished");
    assert!(recorded(dir.path()));
    // Where a new user has something to do, as the C# `OnOnboardingCompleted`.
    assert!(!shell.settings_shown(), "the Sessions page");
    assert_eq!(shell.app().sidebar_tab(), SidebarTab::Sessions);
}

#[test]
fn the_steps_about_the_sessions_show_their_page_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    shell.offer_tour_on_first_run();
    assert!(
        !shell.settings_shown(),
        "the first step is about the sessions"
    );
    for _ in 0..3 {
        let _ = shell.update(Message::Tour(TourMessage::Next));
    }
    assert_eq!(
        shell.tour().map(heimdall_ui::onboarding::Tour::step),
        Some(TourStep::Tools)
    );
    // The steps that name no page leave the one shown, as the C# steps with no `ShellTab`.
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::Tour(TourMessage::Next));
    assert!(shell.settings_shown());
}

#[test]
fn enter_is_next_and_escape_skips_while_nothing_is_over_the_tour() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = started(dir.path());
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(
        shell.tour().map(heimdall_ui::onboarding::Tour::step),
        Some(TourStep::Add)
    );
    // Quick Connect over the tour takes Escape first.
    let _ = shell.update(Message::Shortcut(
        heimdall_ui::terminal_view::keys::WindowShortcut::QuickConnect,
    ));
    let _ = shell.update(Message::EscapeUntaken);
    assert!(shell.tour().is_some(), "Escape closed Quick Connect only");
    let _ = shell.update(Message::EscapeUntaken);
    assert!(
        shell.tour().is_none(),
        "then the tour, as the C# EscapeAsync"
    );
    assert!(recorded(dir.path()));
}

#[test]
fn show_the_tour_again_shows_it_from_its_first_step() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = started(dir.path());
    click(&mut shell, SKIP);
    assert!(shell.tour().is_none());

    // The last card of the General tab, found by its words as the C# search finds it.
    assert_eq!(SettingRow::Onboarding.card(), SettingsCard::Onboarding);
    assert_eq!(
        SettingsCard::Onboarding.tab(),
        heimdall_ui::shell::SettingsTab::General
    );
    let _ = shell.update(Message::ShowSettings);
    assert_eq!(
        shell.settings_found("welcome tour"),
        [SettingRow::Onboarding]
    );
    let messages: Vec<Message> = {
        let mut ui = simulator_at(&shell, SETTINGS_WINDOW);
        ui.find("Welcome tour").expect("the card's heading");
        ui.click(REPLAY).expect(REPLAY);
        ui.into_messages().collect()
    };
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ReplayTour)),
        "{messages:?}"
    );
    for message in messages {
        let _ = shell.update(message);
    }
    assert_eq!(
        shell.tour().map(heimdall_ui::onboarding::Tour::step),
        Some(TourStep::Sessions)
    );
    assert!(!shell.settings_shown(), "its first step's page shown");
    simulator(&shell).find(FIRST_TITLE).expect("shown again");
    // Seen already: replaying it changes nothing on disk until it ends again.
    assert!(recorded(dir.path()));
    click(&mut shell, SKIP);
    assert!(shell.tour().is_none());
}

#[test]
fn an_end_that_cannot_be_saved_keeps_the_tour_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = started(dir.path());
    // A folder where the settings file goes: it cannot be written.
    std::fs::create_dir(settings_path(&dir.path().join("profiles.toml"))).expect("folder");
    click(&mut shell, SKIP);
    assert!(
        shell.tour().is_some(),
        "kept, as the C# CompleteAsync keeps it"
    );
    assert!(!shell.app().onboarding_completed());
    simulator(&shell)
        .find(SAVE_FAILED)
        .expect("said on the card");
}
